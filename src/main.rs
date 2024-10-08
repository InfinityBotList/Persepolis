use std::sync::Arc;

use log::{error, info};
use poise::serenity_prelude::{GuildId, FullEvent};
use sqlx::{postgres::PgPoolOptions, PgPool};

use botox::cache::CacheHttpImpl;

mod admin;
mod checks;
mod cmds;
mod config;
mod finish;
mod help;
mod server;
mod setup;
mod states;
mod stats;
mod perms;

type Error = Box<dyn std::error::Error + Send + Sync>;
type Context<'a> = poise::Context<'a, Data, Error>;

// User data, which is stored and accessible in all command invocations
pub struct Data {
    pool: sqlx::PgPool,
}

async fn clean_out(pool: PgPool, cache_http: CacheHttpImpl) -> ! {
    let mut interval = tokio::time::interval(std::time::Duration::from_secs(30));
    loop {
        interval.tick().await;

        if let Err(e) = clean_out_impl(&pool, &cache_http).await {
            error!("Error while cleaning out: {}", e);
        }
    }
}

async fn clean_out_impl(pool: &PgPool, cache_http: &CacheHttpImpl) -> Result<(), Error> {
    let rows = sqlx::query!(
        "
SELECT id, user_id, guild_id FROM staff_onboardings
-- The guild in question should never be pending manager review
WHERE state != $1
-- Nor complete (!= $2)
AND state != $2
-- And has been created more than three hours ago
AND created_at < NOW() - INTERVAL '3 hours'
        ",
        states::OnboardState::PendingManagerReview.to_string(),
        states::OnboardState::Completed.to_string()
    )
    .fetch_all(pool)
    .await?;

    for row in rows {
        sqlx::query!(
            "DELETE FROM staff_onboardings WHERE id = $1",
            row.id
        )
        .execute(pool)
        .await?;

        let guild_id = row.guild_id.parse::<GuildId>()?;

        setup::delete_or_leave_guild(cache_http, guild_id).await?;
    }

    Ok(())
}

#[poise::command(prefix_command)]
async fn register(ctx: Context<'_>) -> Result<(), Error> {
    poise::builtins::register_application_commands_buttons(ctx).await?;
    Ok(())
}

async fn on_error(error: poise::FrameworkError<'_, Data, Error>) {
    // This is our custom error handler
    // They are many errors that can occur, so we only handle the ones we want to customize
    // and forward the rest to the default handler
    match error {
        poise::FrameworkError::Command { error, ctx, ..  } => {
            error!("Error in command `{}`: {:?}", ctx.command().name, error,);
            let err = ctx
                .say(format!(
                    "There was an error running this command: {}",
                    error
                ))
                .await;

            if let Err(e) = err {
                error!("SQLX Error: {}", e);
            }
        }
        poise::FrameworkError::CommandCheckFailed { error, ctx, .. } => {
            error!(
                "[Possible] error in command `{}`: {:?}",
                ctx.command().name,
                error,
            );
            if let Some(error) = error {
                error!("Error in command `{}`: {:?}", ctx.command().name, error,);
                let err = ctx.say(format!("**{}**", error)).await;

                if let Err(e) = err {
                    error!("Error while sending error message: {}", e);
                }
            }
        }
        error => {
            if let Err(e) = poise::builtins::on_error(error).await {
                error!("Error while handling error: {}", e);
            }
        }
    }
}

async fn event_listener<'a>(
    ctx: poise::FrameworkContext<'a, Data, Error>,
    event: &FullEvent,
) -> Result<(), Error> {
    let user_data = ctx.serenity_context.data::<Data>();

    match event {
        FullEvent::InteractionCreate {
            interaction,
        } => {
            info!("Interaction received: {:?}", interaction.id());
        }
        FullEvent::Ready {
            data_about_bot,
        } => {
            info!(
                "{} is ready!",
                data_about_bot.user.name
            );

            sqlx::query!(
                "UPDATE bots SET type = 'testbot' WHERE bot_id = $1",
                crate::config::CONFIG.test_bot.to_string()
            )
            .execute(&user_data.pool)
            .await?;

            let cache_http_server = CacheHttpImpl::from_ctx(ctx.serenity_context);
            tokio::task::spawn(server::api::setup_server(
                user_data.pool.clone(),
                cache_http_server,
            ));

            let cache_http_cleanout = CacheHttpImpl::from_ctx(ctx.serenity_context);
            tokio::task::spawn(clean_out(
                user_data.pool.clone(),
                cache_http_cleanout,
            ));
        }
        _ => {}
    }

    Ok(())
}

#[tokio::main]
async fn main() {
    const MAX_CONNECTIONS: u32 = 3; // max connections to the database, we don't need too many here

    std::env::set_var("RUST_LOG", "persepolis=info");

    env_logger::init();

    info!("Proxy URL: {}", config::CONFIG.proxy_url);

    let http = serenity::all::HttpBuilder::new(&config::CONFIG.token)
        .proxy(config::CONFIG.proxy_url.clone())
        .ratelimiter_disabled(true)
        .build();

    let client_builder =
        serenity::all::ClientBuilder::new_with_http(Arc::new(http), serenity::all::GatewayIntents::all());

    let data = Data {
        pool: PgPoolOptions::new()
        .max_connections(MAX_CONNECTIONS)
        .connect(&config::CONFIG.database_url)
        .await
        .expect("Could not initialize connection")
    };

    let framework = poise::Framework::new(
        poise::FrameworkOptions {
            initialize_owners: true,
            prefix_options: poise::PrefixFrameworkOptions {
                prefix: Some("ibo!".into()),
                ..poise::PrefixFrameworkOptions::default()
            },
            event_handler: |ctx, event| Box::pin(event_listener(ctx, event)),
            commands: vec![
                register(),
                checks::test_onboardable(),
                checks::test_setup_onboarding(),
                help::help(),
                help::simplehelp(),
                admin::guild(),
                admin::admin(),
                stats::stats(),
                cmds::start::start(),
                cmds::queue::queue(),
                cmds::claim::claim(),
                cmds::approve::approve(),
                cmds::deny::deny(),
                cmds::staffguide::staffguide(),
            ],
            // This code is run before every command
            pre_command: |ctx| {
                Box::pin(async move {
                    info!(
                        "Executing command {} for user {} ({})...",
                        ctx.command().qualified_name,
                        ctx.author().name,
                        ctx.author().id
                    );
                })
            },
            // This code is run after every command returns Ok
            post_command: |ctx| {
                Box::pin(async move {
                    info!(
                        "Done executing command {} for user {} ({})...",
                        ctx.command().qualified_name,
                        ctx.author().name,
                        ctx.author().id
                    );
                })
            },
            on_error: |error| Box::pin(on_error(error)),
            ..Default::default()
        },
    );

    let mut client = client_builder
        .framework(framework)
        .data(Arc::new(data))
        .await
        .expect("Error creating client");

    if let Err(why) = client.start().await {
        error!("Client error: {:?}", why);
    }
}
