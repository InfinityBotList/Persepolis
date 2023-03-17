use poise::serenity_prelude::{Message, GuildId, CreateChannel, CreateInvite, EditMessage, CreateEmbed, CreateActionRow, CreateButton, UserId, EditRole, permissions};
use serenity::json::json;
use crate::{Context, Error, crypto::gen_random, cache::CacheHttpImpl, config};

pub async fn setup_guild(ctx: Context<'_>, msg: &mut Message) -> Result<(), Error> {
    let guild = ctx.discord().http.create_guild(&json!({
        "name": "IBLO-".to_string() + &gen_random(6),
        "channels": [
            {
                "name": "readme",
                "type": 0,
                "topic": "It is recommended that you read this channel before doing anything else."
            }
        ]
    })).await?;

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

pub async fn create_invite(cache_http: &CacheHttpImpl, guild: GuildId) -> Result<String, Error> {
    // Find the readme channel
    let mut readme_channel = None;
    {
        let guild_cache = cache_http.cache.guild(guild).ok_or("Could not find the guild!")?;

        if let Some(chan) = guild_cache.channels.values().find(|c| c.name == "readme") {
            readme_channel = Some(chan.id);
        }
    }

    if readme_channel.is_none() {
        let new_readme_channel = guild.create_channel(cache_http, CreateChannel::new("readme")).await?;

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

    let create_invite = CreateInvite::new()
    .max_age(0)
    .max_uses(0)
    .temporary(false)
    .unique(true);

    let invite = readme_channel.ok_or("Could not unwrap readme channel")?.create_invite(cache_http, create_invite).await?;

    Ok(invite.url())
}

pub async fn promote_user(cache_http: &CacheHttpImpl, guild: GuildId, user: UserId) -> Result<(), Error> {
    let mut admin_role = None;

    {
        let guild_cache = cache_http.cache.guild(guild).ok_or("Could not find the guild!")?;

        if let Some(r) = guild_cache.roles.values().find(|c| c.name == "onboard-user") {
            admin_role = Some(r.id);
        }
    }

    if let Some(role) = admin_role {
        cache_http.http.add_member_role(guild, user, role, Some("Onboarder has joined")).await?;
    } else {
        let new_role = guild.create_role(
            cache_http, 
            EditRole::new()
            .name("onboard-user")
            .permissions(permissions::Permissions::all())
        ).await?;

        cache_http.http.add_member_role(guild, user, new_role.id, Some("Onboarder has joined [new role]")).await?;
    }

    Ok(())
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