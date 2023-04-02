use std::{sync::Arc, num::NonZeroU64};

use axum::{Router, routing::get, extract::{State, Path, Query}, response::{Redirect, IntoResponse, Response}, http::{StatusCode}};
use log::info;
use poise::serenity_prelude::{UserId, GuildId, AddMember};
use serde::Deserialize;
use sqlx::PgPool;
use serenity::json::json;
use tower_http::cors::{CorsLayer, Any};

use crate::{cache::CacheHttpImpl, config, setup::{setup_readme, get_onboard_user_role}};

pub struct AppState {
    pub cache_http: CacheHttpImpl,
    pub pool: PgPool,
}

pub async fn setup_server(pool: PgPool, cache_http: CacheHttpImpl) {
    let shared_state = Arc::new(AppState { pool, cache_http });

    let app = Router::new()
        .route("/:uid", get(create_login))
        .route("/:uid/code", get(get_onboard_code))
        .route("/confirm-login", get(confirm_login))
        .with_state(shared_state)
        .layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods(Any)
                .allow_headers(Any),
        );

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

enum ServerResponse {
    Response(String)
}

impl IntoResponse for ServerResponse {
    fn into_response(self) -> Response {
        match self {
            ServerResponse::Response(e) => {
                (StatusCode::OK, e).into_response()
            }
        }
    }
}

async fn create_login(
    State(app_state): State<Arc<AppState>>,
    Path(uid): Path<UserId>,
) -> Redirect {
    // Redirect user to the login page
    let url = format!("https://discord.com/api/oauth2/authorize?client_id={}&redirect_uri={}/confirm-login&scope={}&state={}&response_type=code", app_state.cache_http.cache.current_user().id, config::CONFIG.persepolis_domain, "identify guilds.join", uid);

    Redirect::temporary(&url)
}

#[derive(Deserialize)]
struct AccessToken {
    access_token: String,
    scope: String,
}

#[derive(Deserialize)]
struct ConfirmLogin {
    code: String,
    state: UserId,
}

async fn confirm_login(
    State(app_state): State<Arc<AppState>>,
    data: Query<ConfirmLogin>,
) -> Result<Redirect, ServerError> {
    // Create access token from code
    let client = reqwest::Client::new();

    let access_token = client.post("https://discord.com/api/v10/oauth2/token")
    .form(
        &json!({
            "client_id": app_state.cache_http.cache.current_user().id.to_string(),
            "client_secret": config::CONFIG.client_secret,
            "grant_type": "authorization_code",
            "code": data.code,
            "redirect_uri": format!("{}/confirm-login", config::CONFIG.persepolis_domain),
        })
    )
    .send()
    .await
    .map_err(|_| ServerError::Error("Could not send request to get access token".to_string()))?
    .error_for_status()
    .map_err(|e| ServerError::Error(
      format!(
        "Could not get access token: {}",
        e
      )
    ))?;

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

    if user.id != data.state {
        // Check if admin
        let staff = sqlx::query!("SELECT admin FROM users WHERE user_id = $1", user.id.to_string())
        .fetch_one(&app_state.pool)
        .await
        .map_err(|_| ServerError::Error("Could not get user from database".to_string()))?;

        if !staff.admin {
            return Err(ServerError::Error("Only 'Staff Managers' and the user themselves can join onboarding servers".to_string()));
        }
    }

    if !access_token.scope.contains("guilds.join") {
        return Err(ServerError::Error("Invalid scope. Scope must be exactly ".to_string()));
    }

    let staff_onboard_guild = sqlx::query!("SELECT staff_onboard_guild FROM users WHERE user_id = $1", data.state.to_string())
    .fetch_one(&app_state.pool)
    .await
    .map_err(|_| ServerError::Error("Could not get guild from database".to_string()))?;

    if let Some(staff_onboard_guild) = staff_onboard_guild.staff_onboard_guild {
        let guild_id = GuildId(staff_onboard_guild.parse::<NonZeroU64>().map_err(|_| ServerError::Error("Could not parse guild id".to_string()))?);
        let channel_id = setup_readme(&app_state.cache_http, guild_id).await.map_err(|_| ServerError::Error("Could not create invite".to_string()))?;

        let guild_url = format!("https://discord.com/channels/{}/{}", guild_id, channel_id);

        // Check that theyre not already on the server
        if app_state.cache_http.cache.member_field(guild_id, user.id, |m| m.user.id).is_some() {
            Ok(Redirect::temporary(&guild_url))
        } else {
            // Add them to server first
            let roles = if user.id == data.state {
                vec![get_onboard_user_role(&app_state.cache_http, guild_id).await.map_err(|_| ServerError::Error("Could not get onboarding roles".to_string()))?]
            } else {
                vec![]
            };

            guild_id.add_member(
                &app_state.cache_http.http, 
                user.id, 
                AddMember::new(access_token.access_token)
                .roles(roles)
            ).await.map_err(|_| ServerError::Error("Could not add you to the guild".to_string()))?;

            Ok(Redirect::temporary(&guild_url))
        }
    } else {
        Err(ServerError::Error("User has no staff onboard guild set".to_string()))
    }
}

#[derive(Deserialize)]
struct GetCode {
    frag: String,
}

async fn get_onboard_code(
    State(app_state): State<Arc<AppState>>,
    Path(uid): Path<UserId>,
    data: Query<GetCode>,
) -> Result<ServerResponse, ServerError> {
    let sess_code = sqlx::query!(
        "SELECT staff_onboard_session_code FROM users WHERE user_id = $1",
        uid.to_string()
    )
    .fetch_one(&app_state.pool)
    .await
    .map_err(|_| ServerError::Error("Could not get user from database".to_string()))?
    .staff_onboard_session_code;

    if let Some(sess_code) = sess_code {
        // Ensure sess_code len is greater than 20
        if sess_code.len() < 20 {
            return Err(ServerError::Error("Internal error: sess_code len > 20".to_string()));
        }

        // Compare first 20 characters
        if sess_code[..20] == data.frag {
            Ok(ServerResponse::Response(sess_code))
        } else {
            Err(ServerError::Error("Invalid code".to_string()))
        }
    } else {
        Err(ServerError::Error("User has no staff onboard code set".to_string()))
    }
}