use poise::serenity_prelude::{Message, GuildId, CreateChannel, CreateInvite};
use serenity::json::json;
use crate::{Context, Error, crypto::gen_random, cache::CacheHttpImpl};

pub async fn setup_guild(ctx: Context<'_>, msg: Message) -> Result<(), Error> {
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

pub async fn delete_or_leave_guild(ctx: Context<'_>, guild: GuildId) -> Result<(), Error> {
    // Since Guild is not Send, it needs to be block-scoped explicitly
    let mut is_owner = false;

    {
        let guild = ctx.discord()
            .cache
            .guild(guild);

        if let Some(guild) = guild {
            is_owner = guild.owner_id == ctx.discord().cache.current_user().id;
        }
    }

    if is_owner {
        // Owner, so delete
        ctx.discord().http.delete_guild(guild).await?;
    } else {
        // We're not owner, so we need to leave
        ctx.discord().http.leave_guild(guild).await?;
    }    

    Ok(())
}