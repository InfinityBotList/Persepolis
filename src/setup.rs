use poise::serenity_prelude::Message;
use serenity::json::json;
use crate::{Context, Error, crypto::gen_random};

pub async fn setup_guild(ctx: Context<'_>, msg: Message) -> Result<(), Error> {
    let guild = ctx.discord().http.create_guild(&json!({
        "name": "IBLO-".to_string() + &gen_random(6),
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