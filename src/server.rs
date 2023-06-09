use std::{collections::HashMap, num::NonZeroU64, sync::Arc};

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
use serde::{Deserialize, Serialize};
use serenity::{all::ChannelId, json::json};
use sqlx::{types::chrono::Utc, PgPool};
use tower_http::cors::{Any, CorsLayer};
use ts_rs::TS;

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
        .route("/quiz", post(create_quiz))
        .route("/resp/:rid", get(get_onboard_response))
        .route("/submit", post(submit_onboarding))
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
    NoContent,
}

impl IntoResponse for ServerResponse {
    fn into_response(self) -> Response {
        match self {
            ServerResponse::Response(e) => (StatusCode::OK, e).into_response(),
            ServerResponse::NoContent => (StatusCode::NO_CONTENT, "").into_response(),
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

#[derive(Serialize, Deserialize, Clone, TS)]
#[ts(export, export_to = ".generated/Verdict.ts")]
pub struct Verdict {
    pub action: String,
    pub reason: String,
    pub end_review_time: i64
}

#[derive(Serialize, Deserialize, Clone, TS)]
#[ts(export, export_to = ".generated/OnboardResponse.ts")]
struct OnboardResponse {
    user_id: String,
    questions: Vec<Question>,
    answer: HashMap<String, String>,
    verdict: Verdict,
    meta: OnboardingMeta,
}

async fn get_onboard_response(
    State(app_state): State<Arc<AppState>>,
    Path(rid): Path<String>,
) -> Result<Json<OnboardResponse>, ServerError> {
    let resp = sqlx::query!(
        "SELECT verdict, questions, answers, meta, user_id FROM onboard_data WHERE onboard_code = $1",
        rid.to_string()
    )
    .fetch_one(&app_state.pool)
    .await
    .map_err(|_| ServerError::Error("Could not find onboarding response".to_string()))?;

    Ok(Json(OnboardResponse {
        user_id: resp.user_id,
        questions: serde_json::from_value(resp.questions)
            .map_err(|_| ServerError::Error("Could not parse qustions".to_string()))?,
        answer: serde_json::from_value(resp.answers)
            .map_err(|_| ServerError::Error("Could not parse answers".to_string()))?,
        verdict: serde_json::from_value(resp.verdict)
            .map_err(|_| ServerError::Error("Could not parse data".to_string()))?,
        meta: serde_json::from_value(resp.meta)
            .map_err(|_| ServerError::Error("Could not parse meta".to_string()))?,
    }))
}

#[derive(Deserialize)]
struct CreateQuizRequest {
    token: String,
    user_id: String,
}

#[derive(Serialize, Deserialize, Clone, TS)]
#[ts(export, export_to = ".generated/PublicQuestion.ts")]
pub struct PublicQuestion {
    pub question: String,
    pub data: QuestionData,
    pub pinned: bool, // Whether or not the question should be pinned/always present in the quiz
}

#[derive(Serialize, TS)]
#[ts(export, export_to = ".generated/CreateQuizResponse.ts")]
struct CreateQuizResponse {
    questions: Vec<PublicQuestion>,
    cached: bool,
}

#[axum_macros::debug_handler]
async fn create_quiz(
    State(app_state): State<Arc<AppState>>,
    Json(create_quiz_req): Json<CreateQuizRequest>,
) -> Result<Json<CreateQuizResponse>, ServerError> {
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
        "SELECT questions FROM onboard_data WHERE onboard_code = $1",
        rec.staff_onboard_current_onboard_resp_id
    )
    .fetch_one(&app_state.pool)
    .await
    .map_err(|_| {
        ServerError::Error("Fatal error: Could not find onboarding response".to_string())
    })?;

    let quiz_ver = resp
        .questions
        .get("quiz_ver")
        .unwrap_or(&json!(0))
        .as_i64()
        .unwrap_or(0);

    if quiz_ver == 1 {
        let obj = json!([]);
        let quiz_qvals = resp.questions.get("questions").unwrap_or(&obj).as_array();

        if let Some(question_vals) = quiz_qvals {
            let mut questions = vec![];

            for q in question_vals {
                // Parse question as Question
                let question: PublicQuestion = serde_json::from_value(q.clone()).map_err(|_| {
                    ServerError::Error("Fatal error: Could not parse question".to_string())
                })?;

                questions.push(question);
            }

            return Ok(Json(CreateQuizResponse {
                questions,
                cached: true,
            }));
        }
    }

    // Create questions randomly from config.questions
    let mut final_questions = vec![];

    // This is in a seperate block to ensure RNG is dropped before saving to database
    {
        let mut mcq_questions = vec![];
        let mut short_questions = vec![];
        let mut long_questions = vec![];

        for q in &config::CONFIG.questions {
            if q.pinned {
                continue; // We add pinned questions later
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

        let mut rng = rand::thread_rng();

        let mcq_choice: Vec<&Question> = mcq_questions
            .choose_multiple(&mut rng, 4)
            .cloned()
            .collect();
        let short_choice: Vec<&Question> = short_questions
            .choose_multiple(&mut rng, 3)
            .cloned()
            .collect();
        let long_choice: Vec<&Question> = long_questions
            .choose_multiple(&mut rng, 2)
            .cloned()
            .collect();

        for q in mcq_choice {
            final_questions.push(q.clone()); // TODO: Try to remove clone
        }

        for q in short_choice {
            final_questions.push(q.clone()); // TODO: Try to remove clone
        }

        for q in long_choice {
            final_questions.push(q.clone()); // TODO: Try to remove clone
        }

        // Add pinned questions
        for q in &config::CONFIG.questions {
            if q.pinned {
                final_questions.push(q.clone()); // TODO: Try to remove clone
            }
        }
    }

    // Save questions to database
    let quiz = json!({
        "questions": final_questions,
        "quiz_ver": 1,
        "cache_nonce": crate::crypto::gen_random(12)
    });

    let id = rec
        .staff_onboard_current_onboard_resp_id
        .ok_or(ServerError::Error(
            "Could not find onboard_resp_id".to_string(),
        ))?;

    sqlx::query!(
        "UPDATE onboard_data SET questions = $1 WHERE onboard_code = $2",
        quiz,
        &id
    )
    .execute(&app_state.pool)
    .await
    .map_err(|_| ServerError::Error("Could not save questions".to_string()))?;

    // Convert final questions to PublicQuestion

    Ok(Json(CreateQuizResponse {
        questions: final_questions
            .iter()
            .map(|q| PublicQuestion {
                question: q.question.clone(),
                data: q.data.clone(),
                pinned: q.pinned,
            })
            .collect::<Vec<PublicQuestion>>(),
        cached: false,
    }))
}

#[derive(Deserialize)]
struct SubmitOnboarding {
    token: String,
    user_id: String,
    quiz_answers: HashMap<String, String>,
    sv_code: String,
}

#[derive(Serialize, Deserialize, Clone, TS)]
#[ts(export, export_to = ".generated/OnboardingMeta.ts")]
pub struct OnboardingMeta {
    pub start_time: i64,
    pub end_time: i64,
}

#[axum_macros::debug_handler]
async fn submit_onboarding(
    State(app_state): State<Arc<AppState>>,
    Json(submit_onboarding_req): Json<SubmitOnboarding>,
) -> Result<ServerResponse, ServerError> {
    let rec = sqlx::query!(
        "SELECT banned, staff_onboard_state, staff_onboard_last_start_time, staff_onboard_guild, staff_onboard_current_onboard_resp_id FROM users WHERE user_id = $1 AND api_token = $2",
        submit_onboarding_req.user_id,
        submit_onboarding_req.token
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

    let id = rec
        .staff_onboard_current_onboard_resp_id
        .ok_or(ServerError::Error(
            "Could not find onboard_resp_id".to_string(),
        ))?;

    // Check onboard_resp with corresponding resp id
    let resp = sqlx::query!(
        "SELECT questions FROM onboard_data WHERE onboard_code = $1",
        id
    )
    .fetch_one(&app_state.pool)
    .await
    .map_err(|_| {
        ServerError::Error("Fatal error: Could not find onboarding response".to_string())
    })?;

    let quiz_ver = resp
        .questions
        .get("quiz_ver")
        .unwrap_or(&json!(0))
        .as_i64()
        .unwrap_or(0);

    if quiz_ver != 1 {
        // Corrupt data, reset questions and error
        sqlx::query!(
            "UPDATE onboard_data SET questions = $1 WHERE onboard_code = $2",
            json!({}),
            &id
        )
        .execute(&app_state.pool)
        .await
        .map_err(|_| ServerError::Error("Could not reset questions".to_string()))?;

        return Err(ServerError::Error(
            "Quiz could not be found and hence has been reset, reload the page and try again"
                .to_string(),
        ));
    }

    let user_id_snow = submit_onboarding_req
        .user_id
        .parse::<NonZeroU64>()
        .map_err(|_| ServerError::Error("Invalid user id".to_string()))?;

    if !crate::finish::check_code(
        &app_state.pool,
        UserId(user_id_snow),
        &submit_onboarding_req.sv_code,
    )
    .await
    .map_err(|e| ServerError::Error(e.to_string()))?
    {
        // Incorrect code
        return Err(ServerError::Error("Incorrect code".to_string()));
    }

    // Next parse the questions in DB
    let obj = json!([]);
    let quiz_qvals = resp.questions.get("questions").unwrap_or(&obj).as_array();

    let mut questions = vec![];

    if let Some(question_vals) = quiz_qvals {
        for q in question_vals {
            // Parse question as Question
            let question: Question = serde_json::from_value(q.clone()).map_err(|_| {
                ServerError::Error("Fatal error: Could not parse question".to_string())
            })?;

            questions.push(question);
        }
    }

    // Now check that every answer is present, adding them to a vec in the order that they have been found
    for question in &questions {
        let answer = submit_onboarding_req
            .quiz_answers
            .get(&question.question)
            .ok_or(ServerError::Error(
                "Missing answer for ".to_string() + &question.question,
            ))?;

        match question.data {
            QuestionData::Short => {
                if answer.len() < 50 {
                    return Err(ServerError::Error(
                        "Short answer questions must be at least 50 characters long".to_string(),
                    ));
                }
            }
            QuestionData::Long => {
                if answer.len() < 750 {
                    return Err(ServerError::Error(
                        "Long answer questions must be at least 750 characters long".to_string(),
                    ));
                }
            }
            QuestionData::MultipleChoice(ref choices) => {
                if !choices.contains(&answer) {
                    return Err(ServerError::Error(
                        "Invalid answer for multiple choice question".to_string(),
                    ));
                }
            }
        }
    }

    // Now we can save the answers
    let mut tx = app_state
        .pool
        .begin()
        .await
        .map_err(|_| ServerError::Error("Could not start transaction".to_string()))?;

    sqlx::query!(
        "UPDATE onboard_data SET questions = $1, answers = $2, meta = $3 WHERE onboard_code = $4",
        serde_json::to_value(questions).map_err(|_| {
            ServerError::Error("Fatal error: Could not serialize questions".to_string())
        })?,
        serde_json::to_value(submit_onboarding_req.quiz_answers)
            .map_err(|_| ServerError::Error("Could not serialize answers".to_string()))?,
        serde_json::to_value(OnboardingMeta {
            start_time: rec
                .staff_onboard_last_start_time
                .ok_or(ServerError::Error(
                    "Could not find last start time".to_string()
                ))?
                .timestamp(),
            end_time: Utc::now().timestamp()
        })
        .map_err(|_| ServerError::Error("Could not serialize meta".to_string()))?,
        &id
    )
    .execute(&mut tx)
    .await
    .map_err(|_| ServerError::Error("Could not save answers".to_string()))?;

    // Set state to PendingManagerReview
    sqlx::query!(
        "UPDATE users SET staff_onboard_state = $1 WHERE user_id = $2",
        crate::states::OnboardState::PendingManagerReview.to_string(),
        submit_onboarding_req.user_id
    )
    .execute(&mut tx)
    .await
    .map_err(|_| ServerError::Error("Could not update state".to_string()))?;

    // Send message on discord
    ChannelId(crate::config::CONFIG.channels.onboarding_channel).say(
        &app_state.cache_http,
        format!(
            "User <@{}> has submitted their onboarding quiz. Please see {}/admin/onboard/resp?id={} to review it, then use the ``/admin approve/deny`` commands to approve or deny it.", 
            submit_onboarding_req.user_id,
            crate::config::CONFIG.frontend_url,
            id
        )
    ).await.map_err(|_| ServerError::Error("Could not send message on discord".to_string()))?;

    tx.commit()
        .await
        .map_err(|_| ServerError::Error("Could not commit transaction".to_string()))?;

    Ok(ServerResponse::NoContent)
}
