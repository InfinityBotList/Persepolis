use log::{error, info};
use poise::serenity_prelude::FullEvent;
use sqlx::postgres::PgPoolOptions;

use crate::cache::CacheHttpImpl;

mod admin;
mod cache;
mod checks;
mod cmds;
mod config;
mod crypto;
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
    cache_http: cache::CacheHttpImpl,
    redis: deadpool_redis::Pool,
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
        poise::FrameworkError::Setup { error, .. } => panic!("Failed to start bot: {:?}", error),
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

async fn event_listener(event: &FullEvent, user_data: &Data) -> Result<(), Error> {
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

            tokio::task::spawn(server::setup_server(
                user_data.pool.clone(),
                user_data.redis.clone(),
                user_data.cache_http.clone(),
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
        serenity::all::ClientBuilder::new_with_http(http, serenity::all::GatewayIntents::all());

    let framework = poise::Framework::new(
        poise::FrameworkOptions {
            initialize_owners: true,
            prefix_options: poise::PrefixFrameworkOptions {
                prefix: Some("ibo!".into()),
                ..poise::PrefixFrameworkOptions::default()
            },
            event_handler: |_ctx, event, _fc, user_data| Box::pin(event_listener(event, user_data)),
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
        move |ctx, _ready, _framework| {
            Box::pin(async move {
                Ok(Data {
                    cache_http: CacheHttpImpl {
                        cache: ctx.cache.clone(),
                        http: ctx.http.clone(),
                    },
                    pool: PgPoolOptions::new()
                        .max_connections(MAX_CONNECTIONS)
                        .connect(&config::CONFIG.database_url)
                        .await
                        .expect("Could not initialize connection"),
                    redis: deadpool_redis::Config::from_url("redis://localhost")
                    .create_pool(Some(deadpool_redis::Runtime::Tokio1)).unwrap(),
                })
            })
        },
    );

    let mut client = client_builder
        .framework(framework)
        .await
        .expect("Error creating client");

    if let Err(why) = client.start().await {
        error!("Client error: {:?}", why);
    }
}
