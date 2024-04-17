use std::str::FromStr;

use poise::{
    serenity_prelude::{CreateEmbed, GuildId},
    CreateReply,
};
use sqlx::types::chrono;

use crate::{
    config,
    setup::{delete_or_leave_guild, setup_guild, setup_readme},
    states, Context, Error, server::types::login::ConfirmLoginState,
};

pub async fn is_admin(ctx: Context<'_>) -> Result<bool, Error> {
    let cmd_name = ctx.invoked_command_name();
    let perms = crate::perms::get_user_perms(&ctx.data().pool, &ctx.author().id.to_string()).await?.resolve();

    if kittycat::perms::has_perm(&perms, &kittycat::perms::build("persepolis", cmd_name)) {
        Ok(true)
    } else {
        Err("You are not an admin".into())
    }
}

pub async fn is_onboardable(ctx: Context<'_>) -> Result<bool, Error> {
    let row = sqlx::query!(
        "SELECT positions FROM staff_members WHERE user_id = $1",
        ctx.author().id.to_string()
    )
    .fetch_optional(&ctx.data().pool)
    .await?;

    if let Some(row) = row {
        if !row.positions.is_empty() {
            return Ok(true);
        }
    }

    let is_staff = {
        let member = botox::cache::member_on_guild(
            ctx,
            config::CONFIG.servers.main,
            ctx.author().id,
            true,
        )
        .await?;

        if let Some(member) = member {
            member
                .roles
                .contains(&config::CONFIG.roles.awaiting_staff)
        } else {
            false
        }
    };

    if is_staff {
        Ok(true)
    } else {
        Err("You are not currently staff nor are you awaiting staff".into())
    }
}

pub async fn setup_onboarding(ctx: Context<'_>) -> Result<bool, Error> {
    // Check f: sqlx::Transaction<'_, sqlx::Postgres>or an existing onboarding session
    let state = sqlx::query!(
        "SELECT state, created_at, guild_id FROM staff_onboardings WHERE user_id = $1 AND void = false AND NOW() - created_at < INTERVAL '3 months' ORDER BY created_at DESC LIMIT 1",
        ctx.author().id.to_string()
    )
    .fetch_optional(&ctx.data().pool)
    .await?;

    let Some(state) = state else {
        // Create a new server
        let mut msg = ctx.send(
            CreateReply::default()
            .embed(
                CreateEmbed::new()
                .title("Onboarding Notice")
                .description(
                    ":yellow_circle: **Creating a new onboarding server for you!**"
                )
                .color(serenity::model::Color::RED)
            )
        ).await?
        .into_message()
        .await?;

        setup_guild(ctx, &mut msg).await?;

        return Ok(false);        
    };

    let onboard_state = states::OnboardState::from_str(&state.state)
        .map_err(|_| "Invalid onboard state")?;

    match onboard_state {
        states::OnboardState::Completed => {
            return Err("You have already completed onboarding! Contact management if you believe this to be an error!".into())
        },
        states::OnboardState::PendingManagerReview => {
            return Err(
                format!("You are currently awaiting manager review! Contact management if you want to check the status on this!

If you accidentally left the onboarding server, you can rejoin using {}
                ", 
                    ConfirmLoginState::JoinOnboardingServer(ctx.author().id).make_login_url(&ctx.cache().current_user().id.to_string()),
                ).into()
            )
        },
        _ => {}
    }

    // Check if older than 3 hours
    if state.created_at.timestamp() + 60*60*3 < chrono::Utc::now().timestamp() {
        // They need to redo onboarding again... wipe their old progress and restart

        let mut msg = ctx.send(
            CreateReply::default()
            .embed(
                CreateEmbed::new()
                .title("Onboarding Notice")
                .description(
                    ":yellow_circle: **Your onboarding session has expired. Starting over...**"
                )
                .color(serenity::model::Color::RED)
            )
        ).await?
        .into_message()
        .await?;

        // Check staff onboard guild
        let guild_id = state
            .guild_id
            .parse::<GuildId>()?;

        let cache_http = botox::cache::CacheHttpImpl::from_ctx(ctx.serenity_context());
        delete_or_leave_guild(&cache_http, guild_id).await?;

        // Delete onboarding
        sqlx::query!(
            "DELETE FROM staff_onboardings WHERE guild_id = $1",
            state.guild_id
        )
        .execute(&ctx.data().pool)
        .await?;

        setup_guild(ctx, &mut msg).await?;

        setup_readme(&cache_http, guild_id)
        .await?;

        return Ok(false);
    }

    // Check that bot is still in guild
    let guild_id = state
        .guild_id
        .parse::<GuildId>()?;

    // This needs to be block-scoped explicitly because Guild is not Send
    let mut in_guild = false;
    {
        let guild = ctx.serenity_context().cache.guild(guild_id);

        if guild.is_some() {
            in_guild = true;
        }
    }

    if !in_guild {
        // Create a new server
        let mut msg = ctx.send(
            CreateReply::default()
            .embed(
                CreateEmbed::new()
                .title("Onboarding Notice")
                .description(
                    ":yellow_circle: **Creating a new onboarding server as the previous one no longer exists!**"
                )
                .color(serenity::model::Color::RED)
            )
        ).await?
        .into_message()
        .await?;

        sqlx::query!(
            "DELETE FROM staff_onboardings WHERE guild_id = $1",
            state.guild_id
        )
        .execute(&ctx.data().pool)
        .await?;

        setup_guild(ctx, &mut msg).await?;

        return Ok(false);
    }

    if guild_id
        != ctx
            .guild_id()
            .ok_or("This command must be ran in a server!")?
    {
        // They're not in the right guild, so we need to ask them to move
        return Err(format!(
            "You are not in the correct guild! Go to {}",
            ConfirmLoginState::JoinOnboardingServer(ctx.author().id).make_login_url(&ctx.cache().current_user().id.to_string()),
        )
        .into());
    }

    Ok(true)
}

#[poise::command(prefix_command, check = "is_onboardable")]
pub async fn test_onboardable(ctx: Context<'_>) -> Result<(), Error> {
    ctx.say("You are *onboardable*!").await?;
    Ok(())
}

#[poise::command(prefix_command, check = "setup_onboarding")]
pub async fn test_setup_onboarding(ctx: Context<'_>) -> Result<(), Error> {
    ctx.say("Pre-run onboarding checks and setup passed!").await?;
    Ok(())
}
