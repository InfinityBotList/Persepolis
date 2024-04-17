use crate::{checks, Context, Error};
use botox::{cache::{CacheHttpImpl, member_on_guild}, crypto::gen_random};
use poise::{
    serenity_prelude::{ButtonStyle, CreateActionRow, CreateButton, CreateMessage, GuildId, User},
    CreateReply,
};
use serenity::builder::{CreateInvite, EditMessage};

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
    let guilds = ctx.serenity_context().cache.guilds();

    let mut guild_list = String::new();

    for guild in guilds.iter() {
        let name = guild
            .name(&ctx.serenity_context().cache)
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
    let gid = guild.parse::<GuildId>()?;

    ctx.serenity_context().http.delete_guild(gid).await?;

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
    let gid = guild.parse::<GuildId>()?;

    ctx.serenity_context().http.leave_guild(gid).await?;

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
        "SELECT id, guild_id FROM staff_onboardings WHERE user_id = $1 AND state = $2 AND NOW() - created_at < INTERVAL '3 hours' ORDER BY created_at DESC LIMIT 1",
        member.id.to_string(),
        crate::states::OnboardState::PendingManagerReview.to_string()
    )
    .fetch_optional(&mut *tx)
    .await?;

    let force = force.unwrap_or(false);

    if let Some(onboard_state) = onboard_state {
        // Update onboard state of user
        sqlx::query!(
            "UPDATE staff_onboardings SET state = $1 WHERE id = $2",
            crate::states::OnboardState::Completed.to_string(),
            onboard_state.id
        )
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;

        let guild_id = onboard_state.guild_id.parse::<GuildId>()?;
        
        crate::setup::delete_or_leave_guild(&CacheHttpImpl::from_ctx(ctx.serenity_context()), guild_id).await?;        
    } else {
        if !force {
            return Err("User does not have any onboardings pending manager review".into());
        }

        sqlx::query!(
            "INSERT INTO staff_onboardings (user_id, guild_id, state) VALUES ($1, $2, $3)",
            member.id.to_string(),
            "force_approved".to_string() + &gen_random(12),
            crate::states::OnboardState::Completed.to_string(),
        )
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;
    }

    // Remove awaiting staff role
    let main_member = member_on_guild(
        &ctx,
        crate::config::CONFIG.servers.main,
        member.id,
        false
    )
    .await?
    .ok_or("Member not found in main server")?;

    if main_member.roles.contains(&crate::config::CONFIG.roles.awaiting_staff) {
        main_member.remove_role(
            &ctx.serenity_context().http,
            crate::config::CONFIG.roles.awaiting_staff,
            Some("Onboarding completed")
        ).await?;
    }

    if !main_member.roles.contains(&crate::config::CONFIG.roles.main_server_web_moderator) {
        main_member.add_role(
            &ctx.serenity_context().http,
            crate::config::CONFIG.roles.main_server_web_moderator,
            Some("Onboarding completed")
        )
        .await?;
    }

    // Create invite in staff server 
    let staff_server_invite = crate::config::CONFIG.channels.onboarding_channel.create_invite(&ctx.serenity_context(), CreateInvite::new().max_uses(1).max_age(0).audit_log_reason("Invite new staff member")).await?;

    // DM user that they have been approved
    let _ = member.dm(
        &ctx.serenity_context().http,
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
    #[description = "Whether or not to force deny. Not recommended unless required"] force: Option<bool>,
) -> Result<(), Error> {
    ctx.defer().await?;

    let data = ctx.data();

    let mut tx = data.pool.begin().await?;

    // Check onboard state of user
    let onboard_state = sqlx::query!(
        "SELECT id, guild_id FROM staff_onboardings WHERE user_id = $1 AND state = $2 AND NOW() - created_at < INTERVAL '3 hours' ORDER BY created_at DESC LIMIT 1",
        user.id.to_string(),
        crate::states::OnboardState::PendingManagerReview.to_string()
    )
    .fetch_optional(&mut *tx)
    .await?;

    let force = force.unwrap_or(false);

    if let Some(onboard_state) = onboard_state {
        // Update onboard state of user
        sqlx::query!(
            "UPDATE staff_onboardings SET state = $1 WHERE id = $2",
            crate::states::OnboardState::Completed.to_string(),
            onboard_state.id
        )
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;

        let guild_id = onboard_state.guild_id.parse::<GuildId>()?;
        
        crate::setup::delete_or_leave_guild(&CacheHttpImpl::from_ctx(ctx.serenity_context()), guild_id).await?;        
    } else {
        if !force {
            return Err("User does not have any onboardings pending manager review".into());
        }

        sqlx::query!(
            "INSERT INTO staff_onboardings (user_id, guild_id, state) VALUES ($1, $2, $3)",
            user.id.to_string(),
            "force_approved".to_string() + &gen_random(12),
            crate::states::OnboardState::Denied.to_string(),
        )
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;
    }

    // DM user that they have been denied
    let _ = user.dm(&ctx.serenity_context().http, CreateMessage::new().content("Your onboarding request has been denied. Please contact a manager for more information")).await?;

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

    let builder = CreateReply::default()
        .content("Are you sure you wish to void all onboardings for this user and force them to redo onboarding?")
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
        .await_component_interaction(ctx.serenity_context().shard.clone())
        .author_id(ctx.author().id)
        .await;

    msg.edit(ctx.serenity_context(), builder.to_prefix_edit(EditMessage::new()).components(vec![]))
        .await?; // remove buttons after button press

    let pressed_button_id = match &interaction {
        Some(m) => &m.data.custom_id,
        None => {
            ctx.say("You didn't interact in time").await?;
            return Ok(());
        }
    };

    if pressed_button_id == "cancel" {
        return Ok(());
    }

    // Update onboard state of a user
    sqlx::query!(
        "UPDATE staff_onboardings SET void = true WHERE user_id = $1",
        user.id.to_string()
    )
    .execute(&data.pool)
    .await?;

    // DM user that they have been force reset
    let _ = user.dm(&ctx.serenity_context().http, CreateMessage::new().content("Your onboarding request has been force reset. Please contact a manager for more information. You will, in most cases, need to redo onboarding")).await?;

    ctx.say("Onboarding request reset!").await?;

    Ok(())
}
