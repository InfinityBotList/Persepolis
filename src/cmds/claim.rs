use std::time::Duration;

use poise::CreateReply;
use poise::serenity_prelude::{
    ButtonStyle,
    CreateActionRow,
    CreateAttachment,
    CreateButton,
    CreateEmbed,
    CreateEmbedFooter,
    CreateWebhook,
    ExecuteWebhook,
    Member,
    Mentionable,
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
pub async fn claim(ctx: Context<'_>, member: Member) -> Result<(), Error> {
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
        crate::states::OnboardState::Started => {
            if member.user.id.0 != crate::config::CONFIG.test_bot {
                ctx.send(
                    CreateReply::new()
                    .embed(
                        CreateEmbed::default()
                        .title("Invalid Bot")
                        .description("You can only claim the test bot!")
                        .color(0xFF0000)
                    )
                ).await?;

                return Ok(());
            }

            let builder = CreateReply::new()
            .embed(
                CreateEmbed::default()
                .title("Bot Already Claimed")
                .description(format!(
                    "This bot is already claimed by {}",
                    data.cache_http.cache.current_user().id.mention()
                ))
                .color(0xFF0000)
            )
            .components(
                vec![
                    CreateActionRow::Buttons(
                        vec![
                            CreateButton::new("fclaim")
                                .label("Force Claim")
                                .style(ButtonStyle::Danger)
                                .disabled(true),
                            CreateButton::new("remind")
                                .label("Remind Reviewer")
                                .style(ButtonStyle::Secondary),
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

            ctx.say("When reviewing, it is STRONGLY recommended (and a good practice) to **remind the reviewer first before force claiming a bot they have claimed**. So, lets do that :smirk:").await?;

            let interaction = msg
            .await_component_interaction(ctx.discord())
            .author_id(ctx.author().id)
            .await;

            msg.edit(ctx.discord(), builder.to_prefix_edit().components(vec![])).await?; // remove buttons after button press

            if let Some(m) = &interaction {
                let id = &m.data.custom_id;

                if id != "remind" {
                    return Ok(());
                }

                ctx.say(
                    format!(
                        "<@{claimed_by}>, did you forgot to finish testing <@{bot_id}>? This reminder has been recorded internally for staff activity tracking purposes!",
                        claimed_by = data.cache_http.cache.current_user().id,
                        bot_id = crate::config::CONFIG.test_bot
                    )
                ).await?;

                // Create a discord webhook
                let wh = ctx
                    .channel_id()
                    .create_webhook(
                        &ctx.discord(),
                        CreateWebhook::new("Splashtail").avatar(
                            &CreateAttachment::url(
                                &ctx.discord(),
                                "https://cdn.infinitybots.xyz/images/png/onboarding-v4.png",
                            )
                            .await?,
                        ),
                    )
                    .await?;

                tokio::time::sleep(Duration::from_secs(3)).await;

                let bot_name = {
                    data.cache_http.cache.user(crate::config::CONFIG.test_bot)
                    .ok_or("Bot not found")?
                    .name
                    .clone()
                };    

                wh.execute(
                    &ctx.discord(),
                    true,
                    ExecuteWebhook::default()
                    .content(
                        format!(
                            "Ack! sorry about that. I completely forgot about {} due to personal issues, yknow?",
                            bot_name
                        )
                    )
                ).await?;
    
                ctx.say("Great! With a real bot, things won't go this smoothly, but you can always remind people to test their bot! Now try claiming again, but this time use ``Force Claim``").await?; 

                sqlx::query!(
                    "UPDATE users SET staff_onboard_state = $1 WHERE user_id = $2",
                    crate::states::OnboardState::QueueRemindedReviewer.to_string(),
                    ctx.author().id.to_string()
                )
                .execute(&data.pool)
                .await?;    
            }

            Ok(())
        },
        crate::states::OnboardState::QueueRemindedReviewer => {
            if member.user.id.0 != crate::config::CONFIG.test_bot {
                ctx.send(
                    CreateReply::new()
                    .embed(
                        CreateEmbed::default()
                        .title("Invalid Bot")
                        .description("You can only claim the test bot!")
                        .color(0xFF0000)
                    )
                ).await?;

                return Ok(());
            }

            let builder = CreateReply::new()
            .embed(
                CreateEmbed::default()
                .title("Bot Already Claimed")
                .description(format!(
                    "This bot is already claimed by {}",
                    data.cache_http.cache.current_user().id.mention()
                ))
                .color(0xFF0000)
            )
            .components(
                vec![
                    CreateActionRow::Buttons(
                        vec![
                            CreateButton::new("fclaim")
                                .label("Force Claim")
                                .style(ButtonStyle::Danger),
                            CreateButton::new("remind")
                                .label("Remind Reviewer")
                                .style(ButtonStyle::Secondary)
                                .disabled(true),
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

                if id != "fclaim" {
                    return Ok(());
                }

                sqlx::query!(
                    "UPDATE users SET staff_onboard_state = $1 WHERE user_id = $2",
                    crate::states::OnboardState::Claimed.to_string(),
                    ctx.author().id.to_string()
                )
                .execute(&data.pool)
                .await?;

                let msg = CreateReply::default().embed(
                    CreateEmbed::default()
                        .title("Bot Claimed")
                        .description(format!("You have claimed <@{}>", crate::config::CONFIG.test_bot))
                        .footer(CreateEmbedFooter::new(
                            "Now you need to start testing it! Listen up...",
                        )),
                );

                ctx.send(msg).await?;

                ctx.say("Before you get to testing the bot, its a good idea to check out the staff guide. To do so, run ``/staffguide`` (or ``ibo!staffguide``).").await?;
            }

            Ok(())
        },
        crate::states::OnboardState::Claimed => {
            Err(
                "You have already claimed the test bot! Please run ``/staffguide`` (``ibo!staffguide``) and then get straight to testing!".into()
            )
        },
        crate::states::OnboardState::Pending => {
            Err(
                format!("Please run ``{}queue`` to get started!", ctx.prefix()).into()
            )
        }
        _ => {
            Err(
                "Hmm... seems like you can't use this command yet!".into()
            )
        } // TODO, remove
    }
}

