use std::{str::FromStr, num::NonZeroU64};

use poise::{serenity_prelude::{RoleId, GuildId, CreateEmbed}, CreateReply};
use sqlx::types::chrono;

use crate::{Context, Error, config, states, setup::{setup_guild, delete_or_leave_guild, create_invite}};

pub async fn onboardable(ctx: Context<'_>) -> Result<bool, Error> {
    let row = sqlx::query!(
        "SELECT staff FROM users WHERE user_id = $1",
        ctx.author().id.to_string()
    )
    .fetch_one(&ctx.data().pool)
    .await?;

    if row.staff {
        return Ok(true)
    }

    let is_staff = {
        let member = ctx.discord()
            .cache
            .member(config::CONFIG.servers.main, ctx.author().id);

        if let Some(member) = member {
            member
                .roles
                .contains(&RoleId(config::CONFIG.roles.awaiting_staff))
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

pub async fn can_onboard(ctx: Context<'_>) -> Result<bool, Error> {
    let state = sqlx::query!(
        "SELECT staff_onboard_state, staff_onboard_last_start_time, staff_onboard_guild FROM users WHERE user_id = $1",
        ctx.author().id.to_string()
    )
    .fetch_one(&ctx.data().pool)
    .await?;

    let onboard_state = states::OnboardState::from_str(&state.staff_onboard_state)
        .map_err(|_| "Invalid onboard state")?;

    if onboard_state == states::OnboardState::Completed {
        return Err("You have already completed onboarding! Contact management if you believe this to be an error!".into())
    }

    // Check if older than 1 hour
    if state.staff_onboard_last_start_time.is_some() {
        let last_start_time = state.staff_onboard_last_start_time.ok_or("Invalid last start time")?;

        if last_start_time.timestamp() + 3600 < chrono::Utc::now().timestamp() {
            // They need to redo onboarding again... wipe their old progress and restart

            let msg = ctx.send(
                CreateReply::new()
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
            if state.staff_onboard_guild.is_some() {
                let guild_id = GuildId(state.staff_onboard_guild.ok_or("Invalid guild ID")?.parse::<NonZeroU64>()?);

                delete_or_leave_guild(ctx, guild_id).await?;
            }

            // Reset to pending
            sqlx::query!(
                "UPDATE users SET staff_onboard_state = $1, staff_onboard_last_start_time = NOW() WHERE user_id = $2",
                states::OnboardState::Pending.to_string(),
                ctx.author().id.to_string()
            )
            .execute(&ctx.data().pool)
            .await?;

            setup_guild(ctx, msg).await?;
        }
    } else if state.staff_onboard_guild.is_some() {
        // Check that bot is still in guild
        let guild_id = GuildId(state.staff_onboard_guild.ok_or("Invalid guild ID")?.parse::<NonZeroU64>()?);

        // This needs to be block-scoped explicitly because Guild is not Send
        let mut in_guild = false;
        {
            let guild = ctx.discord()
                .cache
                .guild(guild_id);

            if let Some(guild) = guild {
                if guild.members.contains_key(&ctx.discord().cache.current_user().id) {
                    // Bot is still in guild, so we can continue
                    in_guild = true;
                }
            }
        }

        if !in_guild {
            // Create a new server
            let msg = ctx.send(
                CreateReply::new()
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
                "UPDATE users SET staff_onboard_state = $1, staff_onboard_last_start_time = NOW() WHERE user_id = $2",
                states::OnboardState::Pending.to_string(),
                ctx.author().id.to_string()
            )
            .execute(&ctx.data().pool)
            .await?;

            setup_guild(ctx, msg).await?;
        }

        if guild_id != ctx.guild_id().ok_or("This command must be ran in a server!")? {
            // They're not in the right guild, so we need to ask them to move
            return Err(
                format!(
                    "You are not in the correct guild! Go to {}/{}>,",
                    config::CONFIG.persepolis_domain,
                    ctx.author().id
                ).into());
        }
    } else {
        // Create a new server
        let msg = ctx.send(
            CreateReply::new()
            .embed(
                CreateEmbed::new()
                .title("Onboarding Notice")
                .description(
                    ":yellow_circle: **Creating a new onboarding server**"
                )
                .color(serenity::model::Color::RED)
            )
        ).await?
        .into_message()
        .await?;

        sqlx::query!(
            "UPDATE users SET staff_onboard_state = $1, staff_onboard_last_start_time = NOW() WHERE user_id = $2",
            states::OnboardState::Pending.to_string(),
            ctx.author().id.to_string()
        )
        .execute(&ctx.data().pool)
        .await?;

        setup_guild(ctx, msg).await?;
    }

    Ok(true)
}

#[poise::command(prefix_command, check = "onboardable")]
pub async fn test_onboardable(ctx: Context<'_>) -> Result<(), Error> {
    ctx.say("You are *onboardable*!").await?;
    Ok(())
}

#[poise::command(prefix_command, check = "can_onboard")]
pub async fn test_can_onboard(ctx: Context<'_>) -> Result<(), Error> {
    ctx.say("Pre-run onboarding checks passed!").await?;
    Ok(())
}