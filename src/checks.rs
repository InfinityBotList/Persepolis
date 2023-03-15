use poise::serenity_prelude::RoleId;

use crate::{Context, Error, config};

async fn onboardable(ctx: Context<'_>) -> Result<bool, Error> {
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

#[poise::command(prefix_command, check = "onboardable")]
pub async fn test(_ctx: Context<'_>) -> Result<(), Error> {
    Ok(())
}