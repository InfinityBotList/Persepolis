use crate::{checks, Context, Error};
use poise::{
    serenity_prelude::{ButtonStyle, CreateActionRow, CreateButton, CreateMessage, GuildId, User},
    CreateReply,
};
use serenity::builder::CreateInvite;
use std::num::NonZeroU64;

/// Guild base command
#[poise::command(
    prefix_command,
    slash_command,
    check = "checks::is_admin",
    subcommands("staff_guildlist", "staff_guilddel", "staff_guildleave")
)]
pub async fn guild(_ctx: Context<'_>) -> Result<(), Error> {
    Ok(())
}

/// Get guild list
#[poise::command(
    rename = "list",
    track_edits,
    prefix_command,
    slash_command,
    check = "checks::is_admin"
)]
pub async fn staff_guildlist(ctx: Context<'_>) -> Result<(), Error> {
    let guilds = ctx.discord().cache.guilds();

    let mut guild_list = String::new();

    for guild in guilds.iter() {
        let name = guild
            .name(ctx.discord())
            .unwrap_or_else(|| "Unknown".to_string())
            + " ("
            + &guild.to_string()
            + ")\n";
        guild_list.push_str(&name);
    }

    ctx.say(&guild_list).await?;

    Ok(())
}

/// Delete server
#[poise::command(
    rename = "del",
    track_edits,
    prefix_command,
    slash_command,
    check = "checks::is_admin"
)]
pub async fn staff_guilddel(
    ctx: Context<'_>,
    #[description = "The guild ID to remove"] guild: String,
) -> Result<(), Error> {
    let gid = guild.parse::<NonZeroU64>()?;

    ctx.discord().http.delete_guild(GuildId(gid)).await?;

    ctx.say("Removed guild").await?;

    Ok(())
}

/// Leave server
#[poise::command(
    rename = "leave",
    track_edits,
    prefix_command,
    slash_command,
    check = "checks::is_admin"
)]
pub async fn staff_guildleave(
    ctx: Context<'_>,
    #[description = "The guild ID to leave"] guild: String,
) -> Result<(), Error> {
    let gid = guild.parse::<NonZeroU64>()?;

    ctx.discord().http.leave_guild(GuildId(gid)).await?;

    ctx.say("Removed guild").await?;

    Ok(())
}

/// Onboarding base command
#[poise::command(
    category = "Admin",
    prefix_command,
    slash_command,
    guild_cooldown = 10,
    subcommands("approveonboard", "denyonboard", "resetonboard",)
)]
pub async fn admin(_ctx: Context<'_>) -> Result<(), Error> {
    Ok(())
}

/// Approve an onboarding
#[poise::command(
    rename = "approve",
    category = "Admin",
    track_edits,
    prefix_command,
    slash_command,
    check = "checks::is_admin"
)]
pub async fn approveonboard(
    ctx: Context<'_>,
    #[description = "The staff id"] member: User,
    #[description = "Whether or not to force approve. Not recommended unless required"] force: Option<bool>,
) -> Result<(), Error> {
    ctx.defer().await?;

    let data = ctx.data();

    let mut tx = data.pool.begin().await?;
    
    // Check onboard state of user
    let onboard_state = sqlx::query!(
        "SELECT staff_onboard_state FROM users WHERE user_id = $1 FOR UPDATE",
        member.id.to_string()
    )
    .fetch_one(&mut *tx)
    .await?;

    let force = force.unwrap_or(false);
    if !force {
        log::info!("Force approving user: {}", member.id);
        if onboard_state.staff_onboard_state
            != crate::states::OnboardState::PendingManagerReview.to_string()
            && onboard_state.staff_onboard_state != crate::states::OnboardState::Denied.to_string()
        {
            return Err(format!(
                "User is not pending manager review and currently has state of: {}",
                onboard_state.staff_onboard_state
            )
            .into());
        }
    }

    // Update onboard state of user
    sqlx::query!(
        "UPDATE users SET staff_onboard_state = $1 WHERE user_id = $2",
        crate::states::OnboardState::Completed.to_string(),
        member.id.to_string()
    )
    .execute(&mut *tx)
    .await?;

    // Remove awaiting staff role
    let mut main_member = ctx.discord().cache.member(crate::config::CONFIG.servers.main, member.id).ok_or("Could not find member in main server")?;

    if main_member.roles.contains(&crate::config::CONFIG.roles.awaiting_staff.into()) {
        main_member.remove_role(
            &ctx.discord().http,
            crate::config::CONFIG.roles.awaiting_staff,
        ).await?;
    }

    if !main_member.roles.contains(&crate::config::CONFIG.roles.main_server_web_moderator.into()) {
        main_member.add_role(
            &ctx.discord().http,
            crate::config::CONFIG.roles.main_server_web_moderator
        )
        .await?;
    }

    // Create invite in staff server 
    let staff_server_invite = {
        let channel = ctx.discord().cache.guild_channel(crate::config::CONFIG.channels.onboarding_channel).ok_or("Could not find onboarding channel")?.clone();

        channel.create_invite(&ctx.discord(), CreateInvite::new().max_uses(1).max_age(0).audit_log_reason("Invite new staff member")).await?
    };

    // DM user that they have been approved
    let _ = member.dm(
        &ctx.discord(),
        CreateMessage::new()
        .content(
            format!("Your onboarding request has been approved. You may now begin approving/denying bots
            
**Note: If you are not yet in the staff server (first timer?), then please first join the `Staff Center` and `Verification Center` servers using the following invite link(s): {} and {}**
            ",
            staff_server_invite.url(),
            crate::config::CONFIG.testing_server
            )
        ) 
    ).await?;

    ctx.say("Onboarding request approved!").await?;

    // Delete the onboarding server
    let staff_onboard_guild = sqlx::query!(
        "SELECT staff_onboard_guild FROM users WHERE user_id = $1",
        member.id.to_string()
    )
    .fetch_one(&mut *tx)
    .await?;

    if let Some(guild) = staff_onboard_guild.staff_onboard_guild {
        if let Ok(guild) = guild.parse::<NonZeroU64>() {
            crate::setup::delete_or_leave_guild(&data.cache_http, GuildId(guild)).await?;
        }
    }

    tx.commit().await?;

    Ok(())
}

/// Denies onboarding requests
#[poise::command(
    rename = "deny",
    category = "Admin",
    track_edits,
    prefix_command,
    slash_command,
    check = "checks::is_admin"
)]
pub async fn denyonboard(
    ctx: crate::Context<'_>,
    #[description = "The staff id"] user: User,
) -> Result<(), Error> {
    let data = ctx.data();

    // Check onboard state of user
    let onboard_state = sqlx::query!(
        "SELECT staff_onboard_state FROM users WHERE user_id = $1",
        user.id.to_string()
    )
    .fetch_one(&data.pool)
    .await?;

    if onboard_state.staff_onboard_state
        != crate::states::OnboardState::PendingManagerReview.to_string()
        && onboard_state.staff_onboard_state != crate::states::OnboardState::Completed.to_string()
    {
        return Err(format!(
            "User is not pending manager review and currently has state of: {}",
            onboard_state.staff_onboard_state
        )
        .into());
    }

    // Update onboard state of user
    sqlx::query!(
        "UPDATE users SET staff_onboard_state = $1 WHERE user_id = $2",
        crate::states::OnboardState::Denied.to_string(),
        user.id.to_string()
    )
    .execute(&data.pool)
    .await?;

    // DM user that they have been denied
    let _ = user.dm(&ctx.discord().http, CreateMessage::new().content("Your onboarding request has been denied. Please contact a manager for more information")).await?;

    ctx.say("Onboarding request denied!").await?;

    Ok(())
}

/// Resets a onboarding to force a new one
#[poise::command(
    rename = "reset",
    category = "Admin",
    track_edits,
    prefix_command,
    slash_command,
    check = "checks::is_admin"
)]
pub async fn resetonboard(
    ctx: crate::Context<'_>,
    #[description = "The staff id"] user: User,
) -> Result<(), Error> {
    let data = ctx.data();

    let builder = CreateReply::new()
        .content("Are you sure you wish to reset this user's onboard state and force them to redo onboarding?")
        .components(
            vec![
                CreateActionRow::Buttons(
                    vec![
                        CreateButton::new("continue").label("Continue").style(ButtonStyle::Primary),
                        CreateButton::new("cancel").label("Cancel").style(ButtonStyle::Danger),
                    ]
                )
            ]
        );

    let mut msg = ctx.send(builder.clone()).await?.into_message().await?;

    let interaction = msg
        .await_component_interaction(ctx.discord())
        .author_id(ctx.author().id)
        .await;

    msg.edit(ctx.discord(), builder.to_prefix_edit().components(vec![]))
        .await?; // remove buttons after button press

    let pressed_button_id = match &interaction {
        Some(m) => &m.data.custom_id,
        None => {
            ctx.say("You didn't interact in time").await?;
            return Ok(());
        }
    };

    if pressed_button_id == "cancel" {
        ctx.say("Cancelled").await?;
        return Ok(());
    }

    // Update onboard state of a user
    sqlx::query!(
        "UPDATE users SET staff_onboard_guild = NULL, staff_onboard_state = $1, staff_onboard_last_start_time = NOW() WHERE user_id = $2",
        crate::states::OnboardState::Pending.to_string(),
        user.id.to_string()
    )
    .execute(&data.pool)
    .await?;

    // DM user that they have been force reset
    let _ = user.dm(&ctx.discord().http, CreateMessage::new().content("Your onboarding request has been force reset. Please contact a manager for more information. You will, in most cases, need to redo onboarding")).await?;

    ctx.say("Onboarding request reset!").await?;

    Ok(())
}
