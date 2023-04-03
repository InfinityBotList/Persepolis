use std::time::Duration;

use poise::{CreateReply, serenity_prelude::{CreateEmbed, Mentionable, CreateEmbedFooter, CreateActionRow, CreateButton, ButtonStyle, CreateWebhook, CreateAttachment, ExecuteWebhook, Member}};

use crate::{checks, Context, Error};

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

#[
    poise::command(
        prefix_command,
        slash_command,
        check = "checks::onboardable",
        check = "checks::can_onboard",
    )
]
pub async fn queue(ctx: Context<'_>) -> Result<(), Error> {
    let data = ctx.data();

    let onboard_state = sqlx::query!(
        "SELECT staff_onboard_state FROM users WHERE user_id = $1",
        ctx.author().id.to_string()
    )
    .fetch_one(&data.pool)
    .await?
    .staff_onboard_state
    .parse::<crate::states::OnboardState>()?;

    let bot_name = {
        data.cache_http.cache.user(crate::config::CONFIG.test_bot)
        .ok_or("Bot not found")?
        .name
        .clone()
    };

    match onboard_state {
        crate::states::OnboardState::Pending => {
            ctx.send(
                CreateReply::new()
                .content(
                    "
**Welcome to Infinity Bot List**

Since you seem new to this place, how about a nice look arou-?                    
                    "
                )
                .embed(
                    CreateEmbed::new()
                    .title("Bot Resubmitted")
                    .description(
                        format!(
                            "**Bot:** <@{bot_id}> ({bot_name})\n\n**Owner:** {owner_id} ({owner_name})\n\n**Bot Page:** {frontend_url}/bots/{bot_id}",
                            bot_id = crate::config::CONFIG.test_bot,
                            bot_name = bot_name,
                            owner_id = data.cache_http.cache.current_user().id.mention(),
                            owner_name = data.cache_http.cache.current_user().name,
                            frontend_url = crate::config::CONFIG.frontend_url,
                        )
                    )
                    .footer(CreateEmbedFooter::new("Are you ready to take on *this* challenge, young padawan?"))
                    .color(0xA020F0)
                )
            ).await?;

            tokio::time::sleep(std::time::Duration::from_secs(3)).await;

            ctx.say("Whoa there! Look at that! There's a new bot to review!!! 

**Here are the general steps to follow:**

1. Type ``/queue`` (or ``ibo!queue``) to see the queue. 
2. Invite the bot to the server (if the invite fails due to lacking verification/anti-spam/whatever, just deny the bot)
3. Then use ``/claim`` (or ``ibo!claim``) to claim the bot.
4. Test the bot in question
5. Approve or deny the bot using ``/approve`` or ``/deny`` (or ``ibo!approve`` or ``ibo!deny``)

**You must complete this challenge within 1 hour. Using testing commands properly will reset the timer.**").await?;

            sqlx::query!(
                "UPDATE users SET staff_onboard_state = $1 WHERE user_id = $2",
                crate::states::OnboardState::Started.to_string(),
                ctx.author().id.to_string()
            )
            .execute(&data.pool)
            .await?;

            Ok(())
        }
        crate::states::OnboardState::Started | crate::states::OnboardState::QueueRemindedReviewer => {
            let bot_name = {
                data.cache_http.cache.user(crate::config::CONFIG.test_bot)
                .ok_or("Bot not found")?
                .name
                .clone()
            };


            let bot_data = sqlx::query!(
                "SELECT short, owner, invite FROM bots WHERE bot_id = $1",
                crate::config::CONFIG.test_bot.to_string()
            )
            .fetch_one(&data.pool)
            .await?;

            let embed = CreateEmbed::new()
            .title(bot_name.to_string() + " [Sandbox Mode]")
            .field("ID", crate::config::CONFIG.test_bot.to_string(), false)
            .field("Short", bot_data.short, false)
            .field("Owner", bot_data.owner.ok_or("Test bot may only have a main owner!")?, false)
            .field("Claimed by", "*You are free to test this bot. It is not claimed*", false)
            .field("Approval Note", "Pls test me and make sure I work :heart:", true)
            .field("Queue name", bot_name, true)
            .field("Invite", format!("[Invite Bot]({})", bot_data.invite), true)
            .footer(CreateEmbedFooter::new("TIP: You can use ibo!claim (or /claim) to claim this bot!"));

            ctx.send(
                CreateReply::new()
                .embed(embed)
                .components(
                    vec![
                        CreateActionRow::Buttons(
                            vec![
                                CreateButton::new_link(bot_data.invite).label("Invite"),
                                CreateButton::new_link(format!("{}/bots/{}", crate::config::CONFIG.frontend_url, crate::config::CONFIG.test_bot)).label("View Page"),
                            ]
                        )
                    ]
                )
            ).await?;

            Ok(())
        },
        _ => {
            let bot_name = {
                data.cache_http.cache.user(crate::config::CONFIG.test_bot)
                .ok_or("Bot not found")?
                .name
                .clone()
            };

            let bot_data = sqlx::query!(
                "SELECT short, owner, invite FROM bots WHERE bot_id = $1",
                crate::config::CONFIG.test_bot.to_string()
            )
            .fetch_one(&data.pool)
            .await?;

            let embed = CreateEmbed::new()
            .title(bot_name.to_string() + " [Sandbox Mode]")
            .field("ID", crate::config::CONFIG.test_bot.to_string(), false)
            .field("Short", bot_data.short, false)
            .field("Owner", bot_data.owner.ok_or("Test bot may only have a main owner!")?, false)
            .field("Claimed by", ctx.author().mention().to_string(), false)
            .field("Approval Note", "Pls test me and make sure I work :heart:", true)
            .field("Queue name", bot_name, true)
            .field("Invite", format!("[Invite Bot]({})", bot_data.invite), true)
            .footer(CreateEmbedFooter::new("TIP: Test this bot now. Then approve/deny it"));

            ctx.send(
                CreateReply::new()
                .embed(embed)
                .components(
                    vec![
                        CreateActionRow::Buttons(
                            vec![
                                CreateButton::new_link(bot_data.invite).label("Invite"),
                                CreateButton::new_link(format!("{}/bots/{}", crate::config::CONFIG.frontend_url, crate::config::CONFIG.test_bot)).label("View Page"),
                            ]
                        )
                    ]
                )
            ).await?;

            Ok(())
        }
    }
}

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
                            "Use ibb!invite or /invite to get the bots invite",
                        )),
                );

                ctx.send(msg).await?;

                ctx.say("Before you get to testing the bot, its a good idea to check out the staff guide. To do so, run ``/staffguide`` (or ``ibo!staffguide``)").await?;
            }

            Ok(())
        },
        crate::states::OnboardState::Pending => {
            Err(
                format!("Please run ``{}queue`` to get started!", ctx.prefix()).into()
            )
        }
        _ => {
            Ok(())
        } // TODO, remove
    }
}

#[
    poise::command(
        prefix_command,
        slash_command,
        check = "checks::onboardable",
        check = "checks::can_onboard",
    )
]
pub async fn unclaim(ctx: Context<'_>) -> Result<(), Error> {
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
        _ => {
            Ok(())
        } // TODO, remove
    }
}

#[
    poise::command(
        prefix_command,
        slash_command,
        check = "checks::onboardable",
        check = "checks::can_onboard",
    )
]
pub async fn approve(ctx: Context<'_>) -> Result<(), Error> {
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
        _ => {
            Ok(())
        } // TODO, remove
    }
}

#[
    poise::command(
        prefix_command,
        slash_command,
        check = "checks::onboardable",
        check = "checks::can_onboard",
    )
]
pub async fn deny(ctx: Context<'_>) -> Result<(), Error> {
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
        _ => {
            Ok(())
        } // TODO, remove
    }
}

#[
    poise::command(
        prefix_command,
        slash_command,
        check = "checks::onboardable",
        check = "checks::can_onboard",
    )
]
pub async fn staffguide(ctx: Context<'_>) -> Result<(), Error> {
    let onboard_code =
        crate::crypto::gen_random(76); // Generate 76 character random string for onboard code

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
",
            url=crate::config::CONFIG.frontend_url,
            uid=ctx.author().id,
            ocf=onboard_fragment
        )
    )
    .await?;

    Ok(())
}