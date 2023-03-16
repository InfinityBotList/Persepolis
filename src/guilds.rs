use std::num::NonZeroU64;

use poise::serenity_prelude::GuildId;

use crate::{checks, Context, Error};


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
    check = "checks::is_admin",
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

/// Delete server
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
