use poise::CreateReply;
use poise::serenity_prelude::{
    CreateEmbed,
    Member,
    CreateActionRow,
    CreateButton,
    ButtonStyle
};

use crate::checks;
use crate::Context;
use crate::Error;

#[
    poise::command(
        prefix_command,
        slash_command,
        check = "checks::onboardable",
        check = "checks::can_onboard",
    )
]
pub async fn deny(ctx: Context<'_>, member: Member, reason: String) -> Result<(), Error> {
    let data = ctx.data();

    let onboard_state = sqlx::query!(
        "SELECT staff_onboard_state FROM users WHERE user_id = $1",
        ctx.author().id.to_string()
    )
    .fetch_one(&data.pool)
    .await?
    .staff_onboard_state
    .parse::<crate::states::OnboardState>()?;

    match onboard_state {
        crate::states::OnboardState::Pending => {
            Err(
                format!("Please run ``{}queue`` to get started!", ctx.prefix()).into()
            )
        }
        crate::states::OnboardState::Claimed => {
            if member.user.id.0 != crate::config::CONFIG.test_bot {
                ctx.send(
                    CreateReply::new()
                    .embed(
                        CreateEmbed::default()
                        .title("Invalid Bot")
                        .description("You can only deny the test bot!")
                        .color(0xFF0000)
                    )
                ).await?;

                return Ok(());
            }

            let builder = CreateReply::new()
            .embed(
                CreateEmbed::default()
                .title("Are you sure?")
                .description("Make sure you've went through the staff guide for our denial criteria and get a good reason before deciding to deny this bot!

In order to better understand your decision, please complete the following survey!                
")
                .color(0xFF0000)
            )
            .components(
                vec![
                    CreateActionRow::Buttons(
                        vec![
                            CreateButton::new("survey")
                                .label("Continue")
                                .style(ButtonStyle::Secondary),
                            CreateButton::new("cancel")
                                .label("Cancel")
                                .style(ButtonStyle::Danger),
                        ]
                    )
                ]
            );

            let mut msg = ctx.send(
                builder.clone()
            )
            .await?
            .into_message()
            .await?;

            Ok(())
        }
        _ => {
            Err(
                "Hmm... seems like you can't use this command yet!".into()
            )
        } // TODO, remove
    }
}
