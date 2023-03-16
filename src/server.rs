use std::{sync::Arc, num::NonZeroU64};

use axum::{Router, routing::get, extract::{State, Path, Query}, response::{Redirect, IntoResponse, Response}, http::{StatusCode}};
use log::info;
use poise::serenity_prelude::{UserId, GuildId};
use serde::Deserialize;
use sqlx::PgPool;
use serenity::json::json;

use crate::{cache::CacheHttpImpl, config, setup::create_invite};

pub struct AppState {
    pub cache_http: CacheHttpImpl,
    pub pool: PgPool,
}

pub async fn setup_server(pool: PgPool, cache_http: CacheHttpImpl) {
    let shared_state = Arc::new(AppState { pool, cache_http });

    let app = Router::new()
        .route("/:uid", get(create_login))
        .route("/confirm-login", get(confirm_login))
        .with_state(shared_state);

    let addr = "127.0.0.1:3011"
        .parse()
        .expect("Invalid server address");

    info!("Starting RPC server on {}", addr);

    if let Err(e) = axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await
    {
        panic!("RPC server error: {}", e);
    }
}

enum ServerError {
    Error(String)
}

impl IntoResponse for ServerError {
    fn into_response(self) -> Response {
        match self {
            ServerError::Error(e) => {
                (StatusCode::BAD_REQUEST, e).into_response()
            }
        }
    }
}

async fn create_login(
    State(app_state): State<Arc<AppState>>,
    Path(uid): Path<UserId>,
) -> Redirect {
    // Redirect user to the login page
    let url = format!("https://discord.com/api/oauth2/authorize?client_id={}&redirect_uri={}/confirm-login&scope={}&state={}", app_state.cache_http.cache.current_user().id, config::CONFIG.persepolis_domain, "identify", uid);

    Redirect::temporary(&url)
}

#[derive(Deserialize)]
struct AccessToken {
    access_token: String,
}

async fn confirm_login(
    State(app_state): State<Arc<AppState>>,
    Query(code): Query<String>,
    Query(state): Query<UserId>,
) -> Result<Redirect, ServerError> {
    // Create access token from code
    let client = reqwest::Client::new();

    let access_token = client.post("https://discord.com/api/v10/token")
    .form(
        &json!({
            "client_id": app_state.cache_http.cache.current_user().id.to_string(),
            "client_secret": config::CONFIG.client_secret,
            "grant_type": "authorization_code",
            "code": code,
            "redirect_uri": format!("{}/confirm-login", config::CONFIG.persepolis_domain),
        })
    )
    .send()
    .await
    .map_err(|_| ServerError::Error("Could not send request to get access token".to_string()))?
    .error_for_status()
    .map_err(|_| ServerError::Error("Invalid code".to_string()))?;

    let access_token = access_token.json::<AccessToken>().await.map_err(|_| ServerError::Error("Could not deserialize response".to_string()))?;

    // Get user from access token
    let user = client.get("https://discord.com/api/v10/users/@me")
    .header("Authorization", format!("Bearer {}", access_token.access_token))
    .send()
    .await
    .map_err(|_| ServerError::Error("Could not send request to get user".to_string()))?
    .error_for_status()
    .map_err(|_| ServerError::Error("Get User failed!".to_string()))?;

    let user = user.json::<serenity::model::user::User>().await.map_err(|_| ServerError::Error("Could not deserialize response".to_string()))?;

    if user.id != state {
        // Check if admin
        let staff = sqlx::query!("SELECT admin FROM users WHERE user_id = $1", user.id.to_string())
        .fetch_one(&app_state.pool)
        .await
        .map_err(|_| ServerError::Error("Could not get user from database".to_string()))?;

        if !staff.admin {
            return Err(ServerError::Error("Only 'Staff Managers' and the user themselves can join onboarding servers".to_string()));
        }
    }

    let staff_onboard_guild = sqlx::query!("SELECT staff_onboard_guild FROM users WHERE user_id = $1", state.to_string())
    .fetch_one(&app_state.pool)
    .await
    .map_err(|_| ServerError::Error("Could not get user from database".to_string()))?;

    if let Some(staff_onboard_guild) = staff_onboard_guild.staff_onboard_guild {
        let guild_id = GuildId(staff_onboard_guild.parse::<NonZeroU64>().map_err(|_| ServerError::Error("Could not parse guild id".to_string()))?);
        let invite_url = create_invite(&app_state.cache_http, guild_id).await.map_err(|_| ServerError::Error("Could not create invite".to_string()))?;

        Ok(Redirect::temporary(&invite_url))
    } else {
        Err(ServerError::Error("User has no staff onboard guild set".to_string()))
    }
}