use crate::{Context, Error, server::types::login::ConfirmLoginState};
use poise::serenity_prelude::{
    ChannelId, CreateActionRow, CreateButton, CreateChannel, CreateEmbed, EditMessage, EditRole,
    GuildId, Mentionable, Message, Permissions, RoleId,
};
use botox::{cache::CacheHttpImpl, crypto::gen_random};
use serde_json::json;
use sqlx::types::uuid;

/// Returns the onboarding id given a context
pub async fn get_onboarding_id(ctx: &Context<'_>) -> Result<Option<uuid::Uuid>, Error> {
    if let Some(guild_id) = ctx.guild_id() {
        let row = sqlx::query!(
            "SELECT id FROM staff_onboardings WHERE guild_id = $1",
            guild_id.to_string()
        )
        .fetch_optional(&ctx.data().pool)
        .await?;

        if let Some(row) = row {
            return Ok(Some(row.id));
        }
    }    

    Ok(None)
}

/// Sets up a guild
pub async fn setup_guild(ctx: Context<'_>, msg: &mut Message) -> Result<(), Error> {
    if ctx.serenity_context().cache.guilds().len() >= 10 {
        return Err(
            "Creating new guilds can only be done when the bot is in less than 10 guilds".into(),
        );
    }

    let guild = ctx
        .serenity_context()
        .http
        .create_guild(&json!({
            "name": "IBLO-".to_string() + &gen_random(6)
        }))
        .await?;

    guild
        .id
        .edit_mfa_level(
            &ctx.serenity_context().http,
            poise::serenity_prelude::MfaLevel::Elevated,
            Some("Onboarding prerequisite"),
        )
        .await
        .map_err(|e| "Could not set MFA level:".to_string() + &e.to_string())?;

    // Update DB
    sqlx::query!(
        "INSERT INTO staff_onboardings (user_id, guild_id) VALUES ($1, $2)",
        ctx.author().id.to_string(),
        guild.id.to_string()
    )
    .execute(&ctx.data().pool)
    .await?;

    // Edit message embed
    msg.edit(
        &ctx.serenity_context(),
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
                            {
                                let url = ConfirmLoginState::JoinOnboardingServer(ctx.author().id).make_login_url(&ctx.cache().current_user().id.to_string());

                                url.clone()
                            }
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
    let mut general_channel = None;
    {
        let guild_cache = cache_http
            .cache
            .guild(guild)
            .ok_or("Could not find the guild!")?;

        for channel in guild_cache.channels.iter() {
            if channel.name == "readme" {
                readme_channel = Some(channel.id);
                continue
            }

            if channel.name == "general" {
                general_channel = Some(channel.id);
                continue
            }
        }
    }

    // Just in case it doesn't exist
    let general_channel = if let Some(chan) = general_channel {
        chan
    } else {
        let new_general_channel = guild
            .create_channel(
                cache_http,
                CreateChannel::new("general")
                .topic("This is the general channel for the server."),
            )
            .await?;

        new_general_channel.id
    };

    if readme_channel.is_none() {
        let new_readme_channel = guild
            .create_channel(
                cache_http,
                CreateChannel::new("readme").topic(
                    "It is recommended that you read this channel before doing anything else.",
                ),
            )
            .await?;

        new_readme_channel.say(
            cache_http,
            format!("
Welcome to your onboarding server! Please read the following:
1. To begin, run ``ibo!queue`` in the {} channel.
2. Make sure to test **all** commands of the test bot during onboarding. In actual bot review, you *do not need to do this* but in onboarding, you **must**.
3. If slash commands do not appear, then try leaving and rejoining, if it still does not work, then please DM staff.

**There is a 3 hour time limit for onboarding and if you exceed this time limit, you will have to start over.**
            ", general_channel.mention())
        ).await?;

        readme_channel = Some(new_readme_channel.id);
    }

    Ok(readme_channel.ok_or("Could not find the readme channel!")?)
}

/// Returns the onboard-user role
pub async fn get_onboard_user_role(
    cache_http: &CacheHttpImpl,
    guild: GuildId,
) -> Result<RoleId, Error> {
    let mut admin_role = None;

    {
        let guild_cache = cache_http
            .cache
            .guild(guild)
            .ok_or("Could not find the guild!")?;

        for role in guild_cache.roles.iter() {
            if role.name == "onboard-user" {
                admin_role = Some(role.id);
                break;
            }
        }
    }

    if let Some(role) = admin_role {
        Ok(role)
    } else {
        let new_role = guild
            .create_role(
                &cache_http.http,
                EditRole::new()
                    .name("onboard-user")
                    .permissions(Permissions::all()),
            )
            .await?;

        Ok(new_role.id)
    }
}

/// Either deletes or leaves the guild
pub async fn delete_or_leave_guild(
    cache_http: impl serenity::all::CacheHttp,
    guild: GuildId,
) -> Result<(), Error> {
    // Since Guild is not Send, it needs to be block-scoped explicitly
    let mut is_owner = false;
    let mut is_in_guild = false;

    let Some(cache) = cache_http.cache() else {
        return Err("Cache not found!".into())
    };
    let http = cache_http.http();

    {
        let guild = cache.guild(guild);

        if let Some(guild) = guild {
            is_in_guild = true;
            is_owner = guild.owner_id == cache.current_user().id;
        }
    }

    if !is_in_guild {
        // We're not in the guild, so we can't do anything
        return Ok(());
    }

    if is_owner {
        // Owner, so delete
        http.delete_guild(guild).await?;
    } else {
        // We're not owner, so we need to leave
        http.leave_guild(guild).await?;
    }

    Ok(())
}
