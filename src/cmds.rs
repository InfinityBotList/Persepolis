use poise::{CreateReply, serenity_prelude::{CreateEmbed, Mentionable, CreateEmbedFooter, CreateActionRow, CreateButton}};

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
                            bot_name = "Ninja Bot",
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

            ctx.say("Whoa there! Look at that! There's a new bot to review!!! Type ``/queue`` (or ``ibb!queue``) to see the queue
            
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
            .field("Invite", format!("[Invite Bot]({})", bot_data.invite), true);

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
pub async fn claim(ctx: Context<'_>) -> Result<(), Error> {
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