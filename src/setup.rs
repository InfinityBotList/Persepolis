use poise::serenity_prelude::{Message, GuildId, CreateChannel, EditMessage, CreateEmbed, CreateActionRow, CreateButton, EditRole, permissions, ChannelId, RoleId};
use serenity::json::json;
use crate::{Context, Error, crypto::gen_random, cache::CacheHttpImpl, config};

pub async fn setup_guild(ctx: Context<'_>, msg: &mut Message) -> Result<(), Error> {
    if ctx.discord().cache.guilds().len() >= 10 {
        return Err("Creating new guilds can only be done when the bot is in less than 10 guilds".into())
    }

    let guild = ctx.discord().http.create_guild(&json!({
        "name": "IBLO-".to_string() + &gen_random(6)
    })).await?;

    guild.id.edit_mfa_level(&ctx.discord().http, poise::serenity_prelude::MfaLevel::Elevated, Some("Onboarding prerequisite"))
    .await
    .map_err(|e| "Could not set MFA level:".to_string() + &e.to_string())?;

    // Update DB
    sqlx::query!(
        "UPDATE users SET staff_onboard_guild = $1 WHERE user_id = $2",
        guild.id.0.to_string(),
        ctx.author().id.to_string()
    )
    .execute(&ctx.data().pool)
    .await?;

    // Edit message embed
    msg.edit(
        &ctx.discord(), 
        EditMessage::new()
        .embed(
            CreateEmbed::new()
            .title("Onboarding Notice")
            .description(
                ":green_circle: **Created onboarding server, now click the 'Join' button to get started!**"
            )
            .color(serenity::model::Color::RED)
        )
        .components(
            vec![
                CreateActionRow::Buttons(
                    vec![
                        CreateButton::new_link(
                            format!(
                                "{}/{}",
                                config::CONFIG.persepolis_domain,
                                ctx.author().id
                            )
                        )
                        .label("Join")
                    ]
                )
            ]
        )
    ).await?;

    Ok(())
}

/// Setups up the readme returning the channel id
pub async fn setup_readme(cache_http: &CacheHttpImpl, guild: GuildId) -> Result<ChannelId, Error> {
    // Find the readme channel
    let mut readme_channel = None;
    {
        let guild_cache = cache_http.cache.guild(guild).ok_or("Could not find the guild!")?;

        if let Some(chan) = guild_cache.channels.values().find(|c| c.name == "readme") {
            readme_channel = Some(chan.id);
        }
    }

    if readme_channel.is_none() {
        let new_readme_channel = guild.create_channel(
            cache_http, 
            CreateChannel::new("readme")
            .topic("It is recommended that you read this channel before doing anything else.")
        ).await?;

        new_readme_channel.say(
            cache_http,
            "
Welcome to your onboarding server! Please read the following:
1. To start onboarding, run ``ibb!onboard`` in the #general channel.
2. There is a 1 hour time limit for onboarding. If you exceed this time limit, you will have to start over. You can extend this limit by progressing through onboarding.
            "
        ).await?;

        readme_channel = Some(new_readme_channel.id);
    }

    Ok(readme_channel.ok_or("Could not find the readme channel!")?)
}

/// Returns the onboard-user role
pub async fn get_onboard_user_role(cache_http: &CacheHttpImpl, guild: GuildId) -> Result<RoleId, Error> {
    let mut admin_role = None;

    {
        let guild_cache = cache_http.cache.guild(guild).ok_or("Could not find the guild!")?;

        if let Some(r) = guild_cache.roles.values().find(|c| c.name == "onboard-user") {
            admin_role = Some(r.id);
        }
    }

    if let Some(role) = admin_role {
        Ok(role)
    } else {
        let new_role = guild.create_role(
            cache_http, 
            EditRole::new()
            .name("onboard-user")
            .permissions(permissions::Permissions::all())
        ).await?;

        Ok(new_role.id)
    }
}

pub async fn delete_or_leave_guild(cache_http: &CacheHttpImpl, guild: GuildId) -> Result<(), Error> {
    // Since Guild is not Send, it needs to be block-scoped explicitly
    let mut is_owner = false;
    let mut is_in_guild = false;

    {
        let guild = cache_http
            .cache
            .guild(guild);

        if let Some(guild) = guild {
            is_in_guild = true;
            is_owner = guild.owner_id == cache_http.cache.current_user().id;
        } 
    }

    if !is_in_guild {
        // We're not in the guild, so we can't do anything
        return Ok(());
    }

    if is_owner {
        // Owner, so delete
        cache_http.http.delete_guild(guild).await?;
    } else {
        // We're not owner, so we need to leave
        cache_http.http.leave_guild(guild).await?;
    }    

    Ok(())
}