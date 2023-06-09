use poise::serenity_prelude::{CreateEmbed, Member};
use poise::CreateReply;
use serenity::all::UserId;
use sqlx::types::chrono::{Utc};

use crate::checks;
use crate::Context;
use crate::Error;

#[poise::command(
    prefix_command,
    slash_command,
    check = "checks::onboardable",
    check = "checks::can_onboard"
)]
pub async fn approve(ctx: Context<'_>, member: Member, reason: String) -> Result<(), Error> {
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
            Err(format!("Please run ``{}queue`` to get started!", ctx.prefix()).into())
        }
        crate::states::OnboardState::Claimed => {
            if member.user.id.0 != crate::config::CONFIG.test_bot {
                ctx.send(
                    CreateReply::new().embed(
                        CreateEmbed::default()
                            .title("Invalid Bot")
                            .description("You can only approve the test bot!")
                            .color(0xFF0000),
                    ),
                )
                .await?;

                return Ok(());
            }

            if reason.len() < 30 {
                ctx.send(
                    CreateReply::new().embed(
                        CreateEmbed::default()
                            .title("Invalid Reason")
                            .description(
                                "Please provide a reason that is at least 30 characters long!",
                            )
                            .color(0xFF0000),
                    ),
                )
                .await?;

                return Ok(());
            }

            /*
            if !crate::finish::check_code(&data.pool, ctx.author().id, code).await? {
                qm.interaction.create_response(&ctx.discord(), CreateInteractionResponse::Message(
                    CreateInteractionResponseMessage::default()
                    .content("Whoa there! You inputted the wrong verification code (hint: ``/staffguide`` or ``ibb!staffguide``)")
                )).await?;

                return Ok(());
            } */

            let mut tx = data.pool.begin().await?;

            let tok = crate::crypto::gen_random(48);
            sqlx::query!(
                "INSERT INTO onboard_data (user_id, onboard_code, verdict) VALUES ($1, $2, $3)",
                ctx.author().id.to_string(),
                tok,
                serde_json::json!({
                    "action": "approve",
                    "reason": reason,
                    "end_review_time": Utc::now().timestamp(), // Current time review ended
                })
            )
            .execute(&mut tx)
            .await?;

            sqlx::query!(
                "UPDATE users SET staff_onboard_state = $1, staff_onboard_current_onboard_resp_id = $2 WHERE user_id = $3",
                crate::states::OnboardState::InQuiz.to_string(),
                tok,
                ctx.author().id.to_string(),
            )
            .execute(&mut tx)
            .await?;

            tx.commit().await?;

            // Try kicking the test bot from the server now
            ctx.guild_id()
                .ok_or("Failed to get guild")?
                .kick_with_reason(
                    &ctx.discord(),
                    UserId(crate::config::CONFIG.test_bot),
                    "Activated Paradise Protection Protocol",
                )
                .await?;

            ctx.say("Oh great work in approving this bo-!").await?;

            tokio::time::sleep(std::time::Duration::from_secs(2)).await;

            ctx.say(format!(
                "
*Paradise Protection Protocol activated, deploying defenses!!!*

Oh well, good luck with the quiz: {}/admin/onboard/quiz
                ",
                crate::config::CONFIG.frontend_url
            ))
            .await?;

            Ok(())
        }
        crate::states::OnboardState::InQuiz => Err(format!(
            "
*Paradise Protection Protocol activated*

Visit {}/admin/onboard/quiz to take the quiz!
                ",
            crate::config::CONFIG.frontend_url
        )
        .into()),
        _ => Err("Hmm... seems like you can't use this command yet!".into()), // TODO, remove
    }
}
