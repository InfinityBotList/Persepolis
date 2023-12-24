use crate::checks;
use crate::Context;
use crate::Error;

#[poise::command(
    prefix_command,
    slash_command,
    check = "checks::is_onboardable",
    check = "checks::setup_onboarding"
)]
pub async fn staffguide(ctx: Context<'_>) -> Result<(), Error> {
    let Some(onboarding_id) = crate::setup::get_onboarding_id(&ctx).await? else {
        return Err("Onboarding ID not found for this server?".into());
    };

    ctx.say(
        format!(
            "The staff guide can be found at {url}/onboarding/guide?id={uid}@{id}.

Once that you've read the staff guide through, start testing the bot, then approve/deny it using ``{prefix}approve`` or ``{prefix}deny``
",
            url=crate::config::CONFIG.panel_url,
            uid=ctx.author().id,
            id=onboarding_id,
            prefix=ctx.prefix()
        )
    )
    .await?;

    Ok(())
}
