use std::{num::NonZeroU64, sync::Arc};

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Redirect, Response},
    routing::get,
    routing::post,
    Json, Router,
};
use log::info;
use poise::serenity_prelude::{AddMember, GuildId, UserId};
use rand::seq::SliceRandom;
use serde::Deserialize;
use serenity::json::json;
use sqlx::PgPool;
use tower_http::cors::{Any, CorsLayer};

use crate::{
    cache::CacheHttpImpl,
    config::{self, Question, QuestionData},
    setup::{get_onboard_user_role, setup_readme},
};

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
        .route(
            "/quiz",
            post(create_quiz),
        )
        .route("/resp/:rid", get(get_onboard_response))
        .with_state(shared_state)
        .layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods(Any)
                .allow_headers(Any),
        );

    let addr = "127.0.0.1:3011".parse().expect("Invalid server address");

    info!("Starting RPC server on {}", addr);

    if let Err(e) = axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await
    {
        panic!("RPC server error: {}", e);
    }
}

enum ServerError {
    Error(String),
}

impl IntoResponse for ServerError {
    fn into_response(self) -> Response {
        match self {
            ServerError::Error(e) => (StatusCode::BAD_REQUEST, e).into_response(),
        }
    }
}

enum ServerResponse {
    Response(String),
}

impl IntoResponse for ServerResponse {
    fn into_response(self) -> Response {
        match self {
            ServerResponse::Response(e) => (StatusCode::OK, e).into_response(),
        }
    }
}

async fn create_login(State(app_state): State<Arc<AppState>>, Path(uid): Path<UserId>) -> Redirect {
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

    let access_token = client
        .post("https://discord.com/api/v10/oauth2/token")
        .form(&json!({
            "client_id": app_state.cache_http.cache.current_user().id.to_string(),
            "client_secret": config::CONFIG.client_secret,
            "grant_type": "authorization_code",
            "code": data.code,
            "redirect_uri": format!("{}/confirm-login", config::CONFIG.persepolis_domain),
        }))
        .send()
        .await
        .map_err(|_| ServerError::Error("Could not send request to get access token".to_string()))?
        .error_for_status()
        .map_err(|e| ServerError::Error(format!("Could not get access token: {}", e)))?;

    let access_token = access_token
        .json::<AccessToken>()
        .await
        .map_err(|_| ServerError::Error("Could not deserialize response".to_string()))?;

    // Get user from access token
    let user = client
        .get("https://discord.com/api/v10/users/@me")
        .header(
            "Authorization",
            format!("Bearer {}", access_token.access_token),
        )
        .send()
        .await
        .map_err(|_| ServerError::Error("Could not send request to get user".to_string()))?
        .error_for_status()
        .map_err(|_| ServerError::Error("Get User failed!".to_string()))?;

    let user = user
        .json::<serenity::model::user::User>()
        .await
        .map_err(|_| ServerError::Error("Could not deserialize response".to_string()))?;

    if user.id != data.state {
        // Check if admin
        let staff = sqlx::query!(
            "SELECT admin FROM users WHERE user_id = $1",
            user.id.to_string()
        )
        .fetch_one(&app_state.pool)
        .await
        .map_err(|_| ServerError::Error("Could not get user from database".to_string()))?;

        if !staff.admin {
            return Err(ServerError::Error(
                "Only 'Staff Managers' and the user themselves can join onboarding servers"
                    .to_string(),
            ));
        }
    }

    if !access_token.scope.contains("guilds.join") {
        return Err(ServerError::Error(
            "Invalid scope. Scope must be exactly ".to_string(),
        ));
    }

    let staff_onboard_guild = sqlx::query!(
        "SELECT staff_onboard_guild FROM users WHERE user_id = $1",
        data.state.to_string()
    )
    .fetch_one(&app_state.pool)
    .await
    .map_err(|_| ServerError::Error("Could not get guild from database".to_string()))?;

    if let Some(staff_onboard_guild) = staff_onboard_guild.staff_onboard_guild {
        let guild_id = GuildId(
            staff_onboard_guild
                .parse::<NonZeroU64>()
                .map_err(|_| ServerError::Error("Could not parse guild id".to_string()))?,
        );
        let channel_id = setup_readme(&app_state.cache_http, guild_id)
            .await
            .map_err(|_| ServerError::Error("Could not create invite".to_string()))?;

        let guild_url = format!("https://discord.com/channels/{}/{}", guild_id, channel_id);

        // Check that theyre not already on the server
        if app_state
            .cache_http
            .cache
            .member_field(guild_id, user.id, |m| m.user.id)
            .is_some()
        {
            Ok(Redirect::temporary(&guild_url))
        } else {
            // Add them to server first
            let roles = if user.id == data.state {
                vec![get_onboard_user_role(&app_state.cache_http, guild_id)
                    .await
                    .map_err(|_| {
                        ServerError::Error("Could not get onboarding roles".to_string())
                    })?]
            } else {
                vec![]
            };

            guild_id
                .add_member(
                    &app_state.cache_http.http,
                    user.id,
                    AddMember::new(access_token.access_token).roles(roles),
                )
                .await
                .map_err(|err| {
                    ServerError::Error(
                        "Could not add you to the guild".to_string() + &err.to_string(),
                    )
                })?;

            Ok(Redirect::temporary(&guild_url))
        }
    } else {
        Err(ServerError::Error(
            "User has no staff onboard guild set".to_string(),
        ))
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
    let rec = sqlx::query!(
        "SELECT banned, staff_onboard_session_code FROM users WHERE user_id = $1",
        uid.to_string()
    )
    .fetch_one(&app_state.pool)
    .await
    .map_err(|_| ServerError::Error("Could not get user from database".to_string()))?;

    if rec.banned {
        return Err(ServerError::Error(
            "You are banned from Infinity Bot List".to_string(),
        ));
    }

    let sess_code = rec.staff_onboard_session_code;

    if let Some(sess_code) = sess_code {
        // Ensure sess_code len is greater than 20
        if sess_code.len() < 20 {
            return Err(ServerError::Error(
                "Internal error: sess_code len > 20".to_string(),
            ));
        }

        // Compare first 20 characters
        if sess_code[..20] == data.frag {
            Ok(ServerResponse::Response(sess_code))
        } else {
            Err(ServerError::Error("Invalid code".to_string()))
        }
    } else {
        Err(ServerError::Error(
            "User has no staff onboard code set".to_string(),
        ))
    }
}

async fn get_onboard_response(
    State(app_state): State<Arc<AppState>>,
    Path(rid): Path<String>,
) -> Result<Json<serde_json::Value>, ServerError> {
    let resp = sqlx::query!(
        "SELECT data, user_id FROM onboard_data WHERE onboard_code = $1",
        rid.to_string()
    )
    .fetch_one(&app_state.pool)
    .await
    .map_err(|_| ServerError::Error("Could not find onboarding response".to_string()))?;

    let json = serde_json::json!({
        "user_id": resp.user_id,
        "data": resp.data
    });

    Ok(Json(json))
}

#[derive(Deserialize)]
struct CreateQuizRequest {
    token: String,
    user_id: String,
}

#[axum_macros::debug_handler]
async fn create_quiz(
    State(app_state): State<Arc<AppState>>,
    Json(create_quiz_req): Json<CreateQuizRequest>,
) -> Result<Json<Vec<Question>>, ServerError> {
    let rec = sqlx::query!(
        "SELECT banned, staff_onboard_state, staff_onboard_current_onboard_resp_id FROM users WHERE user_id = $1 AND api_token = $2",
        create_quiz_req.user_id,
        create_quiz_req.token
    )
    .fetch_one(&app_state.pool)
    .await
    .map_err(|_| ServerError::Error("Invalid user/token combination? Consider logging out and logging in again?".to_string()))?;

    if rec.banned {
        return Err(ServerError::Error(
            "You are banned from Infinity Bot List".to_string(),
        ));
    }

    if rec.staff_onboard_state != crate::states::OnboardState::InQuiz.to_string() {
        return Err(ServerError::Error(
            "Paradise Protection Protocol is not enabled right now".to_string(),
        ));
    }

    // Check onboard_resp with corresponding resp id
    let resp = sqlx::query!(
        "SELECT data FROM onboard_data WHERE onboard_code = $1",
        rec.staff_onboard_current_onboard_resp_id
    )
    .fetch_one(&app_state.pool)
    .await
    .map_err(|_| {
        ServerError::Error("Fatal error: Could not find onboarding response".to_string())
    })?;

    // Check for data["questions"]
    let questions = resp.data["questions"].as_array();

    if let Some(question) = questions {
        let mut questions = vec![];

        for question in question {
            // Parse question as Question
            let question: Question = serde_json::from_value(question.clone()).map_err(|_| {
                ServerError::Error("Fatal error: Could not parse question".to_string())
            })?;

            questions.push(question);
        }

        Ok(Json(questions))
    } else {
        // Create questions randomly from config.questions
        let mut final_questions = vec![];

        // This is in a seperate block to ensure RNG is dropped before saving to database
        {
            let mut mcq_questions = vec![];
            let mut short_questions = vec![];
            let mut long_questions = vec![];

            for q in &config::CONFIG.questions {
                if q.pinned {
                    final_questions.push(q.clone());
                } else {
                    match q.data {
                        QuestionData::MultipleChoice { .. } => {
                            mcq_questions.push(q);
                        }
                        QuestionData::Short { .. } => {
                            short_questions.push(q);
                        }
                        QuestionData::Long { .. } => {
                            long_questions.push(q);
                        }
                    }
                }
            }

            // Choose 3 random mcq questions
            if mcq_questions.len() < 3 {
                return Err(ServerError::Error(
                    "Could not find enough mcq questions".to_string(),
                ));
            }

            let mut rng = rand::thread_rng();

            for _ in 0..3 {
                let q = mcq_questions
                    .choose(&mut rng)
                    .ok_or(ServerError::Error("Could not find questions".to_string()))?;
                final_questions.push(q.clone().clone()); // TODO: Try to remove clone
            }

            // Choose 3 random short questions
            if short_questions.len() < 3 {
                return Err(ServerError::Error(
                    "Could not find enough short questions".to_string(),
                ));
            }

            for _ in 0..3 {
                let q = short_questions
                    .choose(&mut rng)
                    .ok_or(ServerError::Error("Could not find questions".to_string()))?;
                final_questions.push(q.clone().clone()); // TODO: Try to remove clone
            }

            // Choose 2 random long questions
            if long_questions.len() < 2 {
                return Err(ServerError::Error(
                    "Could not find enough long questions".to_string(),
                ));
            }

            for _ in 0..2 {
                let q = long_questions
                    .choose(&mut rng)
                    .ok_or(ServerError::Error("Could not find questions".to_string()))?;
                final_questions.push(q.clone().clone()); // TODO: Try to remove clone
            }
        }

        // Save questions to database
        let questions_json = serde_json::to_value(final_questions.clone())
            .map_err(|_| ServerError::Error("Could not serialize questions".to_string()))?;

        let id = rec
            .staff_onboard_current_onboard_resp_id
            .ok_or(ServerError::Error(
                "Could not find onboard_resp_id".to_string(),
            ))?;

        sqlx::query!(
            "UPDATE onboard_data SET data = $1 WHERE onboard_code = $2",
            json!({ "questions": questions_json }),
            &id
        )
        .execute(&app_state.pool)
        .await
        .map_err(|_| ServerError::Error("Could not save questions".to_string()))?;

        Ok(Json(final_questions))
    }
}
