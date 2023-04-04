use poise::serenity_prelude::UserId;
use sqlx::PgPool;

use crate::Error;

pub async fn check_code(
    pool: &PgPool,
    user_id: UserId, 
    inputted_code: &str
) -> Result<bool, Error> {
    let inputted_code = inputted_code.replace(' ', "");

    // Make sure there are no unicode characters
    if inputted_code.chars().any(|c| c.is_ascii_alphanumeric()) {
        return Err("Unicode characters are not allowed".into());
    }

    let code = sqlx::query!(
        "SELECT staff_onboard_session_code FROM users WHERE user_id = $1",
        user_id.to_string()
    )
    .fetch_one(pool)
    .await?;

    if let Some(code) = code.staff_onboard_session_code {
        // Take last 73 characters
        let mut code = code
            .chars()
            .skip(code.len() - 73)
            .collect::<String>();

        code.replace_range(2..3, "r");
        code.replace_range(
            19..20,
            &user_id.0.to_string().chars().next().unwrap_or_default().to_string(),
        );
        code.replace_range(
            21..22,
            &user_id.0.to_string().chars().nth(1).unwrap_or_default().to_string(),
        );
        code.replace_range(
            40..41,
            &user_id.0.to_string().chars().nth(6).unwrap_or_default().to_string(),
        );
        code.replace_range(39..40, "x");

        let code = code.as_bytes();
        let code = ring::digest::digest(&ring::digest::SHA512, code);
        let code = data_encoding::HEXLOWER.encode(code.as_ref());

        // Take last 6 characters
        let code = code.chars().skip(code.len() - 6).collect::<String>();

        Ok(inputted_code == code)
    } else {
        Ok(false)
    }
}