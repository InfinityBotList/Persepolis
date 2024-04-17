use crate::checks;
use crate::Context;
use crate::Error;

#[poise::command(
    category = "Core",
    prefix_command,
    slash_command,
    check = "checks::is_onboardable",
    check = "checks::setup_onboarding"
)]
pub async fn start(ctx: Context<'_>) -> Result<(), Error> {
    ctx.say("Whoa! You've already started lol").await?;
    Ok(())
}
