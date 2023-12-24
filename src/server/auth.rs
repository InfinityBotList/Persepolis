use crate::Error;
use sqlx::PgPool;

use super::types::auth::AuthData;

/// Checks auth, but does not ensure active sessions
pub async fn check_auth(pool: &PgPool, token: &str) -> Result<AuthData, Error> {
    // Delete expired auths
    sqlx::query!("DELETE FROM staffpanel__authchain WHERE state = $1 AND created_at < NOW() - INTERVAL '1 hour'", "persepolis.active")
        .execute(pool)
        .await?;

    let count = sqlx::query!(
        "SELECT COUNT(*) FROM staffpanel__authchain WHERE token = $1 AND state = $2",
        token,
        "persepolis.active"
    )
    .fetch_one(pool)
    .await?
    .count
    .unwrap_or(0);

    if count == 0 {
        return Err("identityExpired".into());
    }

    let rec = sqlx::query!(
        "SELECT user_id, created_at, state FROM staffpanel__authchain WHERE token = $1 AND state = $2",
        token,
        "persepolis.active"
    )
    .fetch_one(pool)
    .await?;

    Ok(AuthData {
        user_id: rec.user_id,
        created_at: rec.created_at.timestamp(),
        state: rec.state,
    })
}
