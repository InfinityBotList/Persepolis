use std::time::Duration;

use poise::CreateReply;
use poise::serenity_prelude::{
    CreateEmbed,
    Member, 
    CreateActionRow,
    CreateButton, 
    ButtonStyle, 
    CreateQuickModal, 
    CreateInputText, 
    InputTextStyle, 
    CreateInteractionResponse, 
    CreateInteractionResponseMessage, 
    ChannelId
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
            Err(
                format!("Please run ``{}queue`` to get started!", ctx.prefix()).into()
            )
        },
        crate::states::OnboardState::Claimed => {
            if member.user.id.0 != crate::config::CONFIG.test_bot {
                ctx.send(
                    CreateReply::new()
                    .embed(
                        CreateEmbed::default()
                        .title("Invalid Bot")
                        .description("You can only approve the test bot!")
                        .color(0xFF0000)
                    )
                ).await?;

                return Ok(());
            }

            if reason.len() < 30 {
                ctx.send(
                    CreateReply::new()
                    .embed(
                        CreateEmbed::default()
                        .title("Invalid Reason")
                        .description("Please provide a reason that is at least 30 characters long!")
                        .color(0xFF0000)
                    )
                ).await?;

                return Ok(());
            }

            let builder = CreateReply::new()
            .embed(
                CreateEmbed::default()
                .title("Are you sure?")
                .description("Make sure you've went through the staff guide and actually tested **every** command for onboarding before deciding to approve this bot!

In order to better understand your decision, please complete the survey by clicking 'Continue'!                
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

            let interaction = msg
            .await_component_interaction(ctx.discord())
            .author_id(ctx.author().id)
            .await;

            msg.edit(ctx.discord(), builder.to_prefix_edit().components(vec![])).await?; // remove buttons after button press

            if let Some(m) = &interaction {
                let id = &m.data.custom_id;

                if id == "cancel" {
                    return Ok(());
                }

                let qm = m.quick_modal(
                    ctx.discord(), 
                    CreateQuickModal::new("Approve Bot")
                    .field(
                        CreateInputText::new(
                            InputTextStyle::Paragraph,
                            "Was the bot generally free of errors",
                            "free_of_errors"
                        )
                        .placeholder("I experienced...")
                        .required(true)
                    )
                    .field(
                        CreateInputText::new(
                            InputTextStyle::Paragraph,
                            "Did all commands tested work as expected?",
                            "tested_commands"
                        )
                        .placeholder("I tested...")
                        .required(true)
                    )
                    .field(
                        CreateInputText::new(
                            InputTextStyle::Paragraph,
                            "Feedback on onboarding",
                            "feedback"
                        )
                        .placeholder("I felt that...")
                        .required(true)
                    )
                    .field(
                        CreateInputText::new(
                        InputTextStyle::Short,
                        "Staff Verify Code",
                        "code",
                        )
                        .placeholder("You can find this by running the staffguide command")
                        .required(true)
                    )
                    .timeout(Duration::from_secs(300))
                )
                .await?;

                if let Some(qm) = qm {
                    let inputs = qm.inputs;

                    let (free_of_errors, tested_commands, feedback, code) = (
                        &inputs[0],
                        &inputs[1],
                        &inputs[2],
                        &inputs[3],
                    );

                    if !crate::finish::check_code(&data.pool, ctx.author().id, code).await? {
                        qm.interaction.create_response(&ctx.discord(), CreateInteractionResponse::Message(
                            CreateInteractionResponseMessage::default()
                            .content("Whoa there! You inputted the wrong verification code (hint: ``/staffguide`` or ``ibb!staffguide``)")
                        )).await?;

                        return Ok(());
                    }

                    let s_onboard = sqlx::query!(
                        "SELECT staff_onboarded, staff_onboard_macro_time FROM users WHERE user_id = $1",
                        ctx.author().id.to_string()
                    )
                    .fetch_one(&data.pool)
                    .await?;

                    let tok = crate::crypto::gen_random(48);
                    sqlx::query!("INSERT INTO onboard_data (user_id, onboard_code, data) VALUES ($1, $2, $3)", 
                        ctx.author().id.to_string(),
                        tok,
                        serde_json::json!({
                            "free_of_errors": free_of_errors,
                            "tested_commands": tested_commands,
                            "feedback": feedback,
                            "approval_reason": reason,
                            "submit_ts": sqlx::types::chrono::Utc::now().timestamp(),
                            "start_ts": s_onboard.staff_onboard_macro_time.unwrap_or_default().timestamp(),
                            "staff_onboarded_before": s_onboard.staff_onboarded,    
                        })
                    )
                    .execute(&data.pool)
                    .await?;

                    ChannelId(crate::config::CONFIG.channels.onboarding_channel).say(
                        &ctx.discord(),
                        format!(
                            "**New onboarding attempt**\n\n**User ID:** {user_id}\n**Action taken:** {action}\n**Overall reason:** {reason}.\n**URL:** {url}",
                            user_id = ctx.author().id,
                            action = "approve",
                            reason = reason,
                            url = crate::config::CONFIG.frontend_url.clone()+"/staff/onboardresp/" + &tok
                        )
                    ).await?;

                    sqlx::query!(
                        "UPDATE users SET staff_onboard_state = $1 WHERE user_id = $2",
                        crate::states::OnboardState::PendingManagerReview.to_string(),
                        ctx.author().id.to_string()
                    )
                    .execute(&data.pool)
                    .await?;

                    qm.interaction.create_response(&ctx.discord(), CreateInteractionResponse::Message(
                        CreateInteractionResponseMessage::default()
                        .content("Now, you just need to wait for a manager to approve this onboarding response!")
                    )).await?;
                } else {
                    return Ok(())
                }
            }

            Ok(())
        }
        _ => {
            Err(
                "Hmm... seems like you can't use this command yet!".into()
            )
        } // TODO, remove
    }
}
