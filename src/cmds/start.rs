use crate::Context;
use crate::Error;
use crate::checks;

#[
    poise::command(
        prefix_command,
        slash_command,
        check = "checks::onboardable",
        check = "checks::can_onboard",
    )
]
pub async fn start(ctx: Context<'_>) -> Result<(), Error> {
    ctx.say("Whoa! You've already started lol").await?;
    Ok(())
}
