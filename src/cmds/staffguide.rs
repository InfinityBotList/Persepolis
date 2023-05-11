use crate::checks;
use crate::Context;
use crate::Error;

#[poise::command(
    prefix_command,
    slash_command,
    check = "checks::onboardable",
    check = "checks::can_onboard"
)]
pub async fn staffguide(ctx: Context<'_>) -> Result<(), Error> {
    let onboard_code = crate::crypto::gen_random(76); // Generate 76 character random string for onboard code

    // Get first 20 characters of the onboard code as onboard_fragment
    let onboard_fragment = onboard_code.chars().take(20).collect::<String>();

    // Set onboard code for user
    sqlx::query!(
        "UPDATE users SET staff_onboard_session_code = $1 WHERE user_id = $2",
        onboard_code,
        ctx.author().id.to_string()
    )
    .execute(&ctx.data().pool)
    .await?;

    ctx.say(
        format!(
            "The staff guide can be found at {url}/staff/guide?svu={uid}@{ocf}. Please **do not** bookmark this page as the URL may change in the future

**Note that during onboarding, the *5 digit staff verify code present somewhere in the guide* will be reset every time you run the ``staffguide`` command! Always use the latest command invocation for getting the code**  

Once that you've read the staff guide through, start testing the bot, then approve/deny it using ``{prefix}approve`` or ``{prefix}deny``
",
            url=crate::config::CONFIG.frontend_url,
            uid=ctx.author().id,
            ocf=onboard_fragment,
            prefix=ctx.prefix()
        )
    )
    .await?;

    Ok(())
}
