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
    let onboard_code = crate::crypto::gen_random(76); // Generate 76 character random string for onboard code

    let Some(onboarding_id) = crate::setup::get_onboarding_id(&ctx).await? else {
        return Err("Onboarding ID not found for this server?".into());
    };

    // Set onboard code for user
    sqlx::query!(
        "UPDATE staff_onboardings SET staff_verify_code = $1 WHERE id = $2 AND user_id = $3",
        onboard_code,
        onboarding_id,
        ctx.author().id.to_string()
    )
    .execute(&ctx.data().pool)
    .await?;

    ctx.say(
        format!(
            "The staff guide can be found at {url}/onboarding/guide?id={uid}@{id}. Please **do not** bookmark this page as the URL may change in the future

**Note that during onboarding, the *5 digit staff verify code present somewhere in the guide* will be reset every time you run the ``staffguide`` command! Always use the latest command invocation for getting the code**  

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
