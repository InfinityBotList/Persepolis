use std::{collections::HashMap, sync::Arc, str::FromStr, fmt::Display};

use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::{IntoResponse, Redirect, Response},
    routing::get,
    routing::post,
    Json, Router,
};
use log::info;
use poise::serenity_prelude::{AddMember, GuildId};
use rand::seq::SliceRandom;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::{PgPool, types::uuid};
use tower_http::cors::{Any, CorsLayer};
use ts_rs::TS;

use botox::cache::{member_on_guild, CacheHttpImpl};

use crate::{
    config::{self, Question, QuestionData},
    setup::{get_onboard_user_role, setup_readme},
};

use super::types::{login::ConfirmLoginState, auth::{GetAuthData, CreateLogin}, oauth2::{ConfirmLogin, AccessTokenResponse}};

struct Error {
    status: StatusCode,
    message: String,
}

impl Error {
    fn new(e: impl Display) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: e.to_string(),
        }
    }
}

impl IntoResponse for Error {
    fn into_response(self) -> Response {
        (self.status, self.message).into_response()
    }
}

pub struct AppState {
    pub cache_http: CacheHttpImpl,
    pub pool: PgPool,
}

pub async fn setup_server(pool: PgPool, cache_http: CacheHttpImpl) {
    let shared_state = Arc::new(AppState { pool, cache_http });

    let app = Router::new()
        .route("/create-login", get(create_login))
        .route("/confirm-login", get(confirm_login))
        .route("/auth-data", post(get_auth_data))
        .route("/onboarding-code", post(get_onboarding_code))
        .route("/quiz", post(create_quiz))
        .route("/onboarding-response", post(get_onboard_response))
        .route("/submit-quiz", post(submit_onboarding))
        .with_state(shared_state)
        .layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods(Any)
                .allow_headers(Any),
        );

    let addr = "0.0.0.0:3011".parse().expect("Invalid server address");

    info!("Starting RPC server on {}", addr);

    if let Err(e) = axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await
    {
        panic!("RPC server error: {}", e);
    }
}


async fn get_auth_data(State(app_state): State<Arc<AppState>>, Json(gad): Json<GetAuthData>) -> Result<impl IntoResponse, Error> {
    let auth_data = super::auth::check_auth(
        &app_state.pool,
        &gad.login_token,
    )
    .await
    .map_err(Error::new)?;

    Ok(Json(auth_data))
}

async fn create_login(State(app_state): State<Arc<AppState>>, Query(cl): Query<CreateLogin>) -> Result<impl IntoResponse, Error> {
    let state = ConfirmLoginState::from_str(&cl.state).map_err(Error::new)?;
        Ok(Redirect::temporary(&state.make_login_url(&app_state.cache_http.cache.current_user().id.to_string())).into_response())
}

async fn confirm_login(
    State(app_state): State<Arc<AppState>>,
    Query(data): Query<ConfirmLogin>,
) -> Result<impl IntoResponse, Error> {
    let state = ConfirmLoginState::from_str(data.state.as_str()).map_err(|_| Error::new("Invalid state"))?;

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
        .map_err(|_| Error::new("Could not send request to get access token".to_string()))?
        .error_for_status()
        .map_err(|e| Error::new(format!("Could not get access token: {}", e)))?;

    let access_token = access_token
        .json::<AccessTokenResponse>()
        .await
        .map_err(|_| Error::new("Could not deserialize response".to_string()))?;

    // Get user from access token
    let user = client
        .get("https://discord.com/api/v10/users/@me")
        .header(
            "Authorization",
            format!("Bearer {}", access_token.access_token),
        )
        .send()
        .await
        .map_err(|_| Error::new("Could not send request to get user".to_string()))?
        .error_for_status()
        .map_err(|_| Error::new("Get User failed!".to_string()))?;

    let user = user
        .json::<serenity::model::user::User>()
        .await
        .map_err(|_| Error::new("Could not deserialize response".to_string()))?;

    // Check if staff member or awaiting staff
    let row = sqlx::query!(
        "SELECT positions FROM staff_members WHERE user_id = $1",
        user.id.to_string()
    )
    .fetch_optional(&app_state.pool)
    .await
    .map_err(|_| Error::new("Could not get staff member data from database".to_string()))?;

    let is_staff = {
        if row.is_some() && !row.unwrap().positions.is_empty() {
            true
        } else {
            let member = member_on_guild(
                &app_state.cache_http,
                config::CONFIG.servers.main,
                user.id,
                false
            )
            .await
            .map_err(|e| Error::new(format!("Failed to fetch member: {:#?}", e)))?;

            if let Some(member) = member {
                member
                    .roles
                    .contains(&config::CONFIG.roles.awaiting_staff)
            } else {
                false
            }
        }
    };

    if !is_staff {
        return Err(Error::new("You are not a staff member or awaiting staff"));
    }

    match state {
        ConfirmLoginState::JoinOnboardingServer(uid) => {
            if user.id != uid {
                // Check if admin
                let perms = crate::perms::get_user_perms(&app_state.pool, &user.id.to_string())
                    .await
                    .map_err(|e| Error::new(format!("Could not get user perms: {}", e)))?
                    .resolve();
        
                if !kittycat::perms::has_perm(&perms, &kittycat::perms::build("persepolis", "join_onboarding_servers")) {
                    return Err(
                        Error::new("Only staff members with the `persepolis.join_onboarding_servers` permission and the user themselves can join onboarding servers")
                    );
                }
            }
        
            if !access_token.scope.contains("guilds.join") {
                return Err(
                    Error::new("Invalid scope. Scope must be exactly contain guilds.join"),
                );
            }
        
            let guild_id = sqlx::query!(
                "SELECT guild_id FROM staff_onboardings WHERE user_id = $1 AND state != $2 ORDER BY created_at DESC LIMIT 1",
                uid.to_string(),
                crate::states::OnboardState::Completed.to_string()
            )
            .fetch_one(&app_state.pool)
            .await
            .map_err(|_| Error::new("Could not get any pending onboarding guilds for you from database"))?;
        
            let guild_id = guild_id.guild_id.parse::<GuildId>().map_err(|e| {
                Error::new(
                    format!("Could not parse guild id {}", e)
                )
            })?;
            let channel_id = setup_readme(&app_state.cache_http, guild_id)
                .await
                .map_err(|_| Error::new("Could not create invite"))?;
        
            let guild_url = format!("https://discord.com/channels/{}/{}", guild_id, channel_id);
        
            // Check that theyre not already on the server
            if member_on_guild(
                &app_state.cache_http,
                guild_id,
                user.id,
                false
            )
            .await
            .map_err(|e| Error::new(format!("Failed to fetch member: {:#?}", e)))?
            .is_some() {
                Ok(Redirect::temporary(&guild_url).into_response())
            } else {
                // Add them to server first
                let roles = if user.id == uid {
                    vec![get_onboard_user_role(&app_state.cache_http, guild_id)
                        .await
                        .map_err(Error::new)?]
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
                        Error::new(
                            format!("Could not add user to guild: {}", err)
                        )
                    })?;
        
                Ok(Redirect::temporary(&guild_url).into_response())
            }
        }
        ConfirmLoginState::CreateSession(url) => {  
            if !url.starts_with(&crate::config::CONFIG.panel_url) && !url.starts_with("http://localhost") {
                return Err(Error::new("Invalid url".to_string()));
            }
     
            info!("Creating session for {}", user.id.to_string());     
            // Create a random number between 4196 and 6000 for the token
            let token = botox::crypto::gen_random(512);

            sqlx::query!(
                "INSERT INTO staffpanel__authchain (user_id, token, popplio_token, state) VALUES ($1, $2, $3, $4)",
                user.id.to_string(),
                token,
                botox::crypto::gen_random(2048),
                "persepolis.active"
            )
            .execute(&app_state.pool)
            .await
            .map_err(|_| Error::new("Could not create session".to_string()))?;

            Ok(Redirect::temporary(
                &format!(
                    "{}?token={}",
                    url,
                    token
                )
            ).into_response())
        }
    }
}

#[derive(Serialize, Deserialize, Clone, TS)]
#[ts(export, export_to = ".generated/GetOnboardingCode.ts")]
struct GetOnboardingCode {
    login_token: String,
    id: String,
}

#[axum_macros::debug_handler]
async fn get_onboarding_code(
    State(app_state): State<Arc<AppState>>,
    Json(get_onboarding_code_req): Json<GetOnboardingCode>,
) -> Result<impl IntoResponse, Error> {
    let auth_data = super::auth::check_auth(
        &app_state.pool,
        &get_onboarding_code_req.login_token,
    )
    .await
    .map_err(Error::new)?;

    let mut tx = app_state
        .pool
        .begin()
        .await
        .map_err(|_| Error::new("Could not start transaction".to_string()))?;

    let uuid = sqlx::types::uuid::Uuid::from_str(&get_onboarding_code_req.id)
        .map_err(|_| Error::new("Invalid id".to_string()))?;

    let rec = sqlx::query!(
        "SELECT user_id, staff_verify_code FROM staff_onboardings WHERE id = $1 AND user_id = $2",
        uuid,
        auth_data.user_id
    )
    .fetch_one(&mut *tx)
    .await
    .map_err(|_| Error::new("Could not find onboarding response".to_string()))?;

    let code = if let Some(code) = rec.staff_verify_code {
        code
    } else {
        // Generate 76 character random string for onboard code
        let onboard_code = botox::crypto::gen_random(76);

        // Set onboard code for user
        sqlx::query!(
            "UPDATE staff_onboardings SET staff_verify_code = $1 WHERE id = $2 AND user_id = $3",
            onboard_code,
            uuid,
            auth_data.user_id
        )
        .execute(&mut *tx)
        .await
        .map_err(|_| Error::new("Could not set onboard code".to_string()))?;

        onboard_code
    };

    tx.commit()
        .await
        .map_err(|_| Error::new("Could not commit transaction".to_string()))?;

    Ok(code.into_response())
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
    questions: Option<Vec<Question>>,
    answers: Option<HashMap<String, String>>,
    verdict: Option<Verdict>,
    created_at: i64,
    finished_at: Option<i64>,
}

#[derive(Serialize, Deserialize, Clone, TS)]
#[ts(export, export_to = ".generated/GetOnboardingResponse.ts")]
struct GetOnboardingResponse {
    login_token: String,
    id: String,
}

async fn get_onboard_response(
    State(app_state): State<Arc<AppState>>,
    Json(req): Json<GetOnboardingResponse>,
) -> Result<Json<OnboardResponse>, Error> {
    let auth_data = super::auth::check_auth(
        &app_state.pool,
        &req.login_token,
    )
    .await
    .map_err(Error::new)?;

    let user_perms = crate::perms::get_user_perms(&app_state.pool, &auth_data.user_id)
        .await
        .map_err(|e| Error::new(format!("Could not get user perms: {}", e)))?
        .resolve();

    if !kittycat::perms::has_perm(&user_perms, &kittycat::perms::build("persepolis", "view_onboarding_responses")) {
        return Err(Error::new("You do not have permission to view onboarding responses".to_string()));
    }

    let uuid = uuid::Uuid::from_str(&req.id)
        .map_err(|_| Error::new("Invalid id".to_string()))?;

    let resp = sqlx::query!(
        "SELECT verdict, questions, answers, created_at, finished_at, user_id FROM staff_onboardings WHERE id = $1",
        uuid
    )
    .fetch_one(&app_state.pool)
    .await
    .map_err(|_| Error::new("Could not find onboarding response".to_string()))?;

    let questions = if let Some(questions) = resp.questions {
        Some(serde_json::from_value::<Vec<Question>>(questions)
            .map_err(|_| Error::new("Could not parse questions".to_string()))?)
    } else {
        None
    };

    let answers = if let Some(answers) = resp.answers {
        Some(serde_json::from_value::<HashMap<String, String>>(answers)
            .map_err(|_| Error::new("Could not parse answers".to_string()))?)
    } else {
        None
    };

    let verdict = if let Some(verdict) = resp.verdict {
        Some(serde_json::from_value::<Verdict>(verdict)
            .map_err(|_| Error::new("Could not parse verdict".to_string()))?)
    } else {
        None
    };

    Ok(Json(OnboardResponse {
        user_id: resp.user_id,
        questions,
        answers,
        verdict,
        created_at: resp.created_at.timestamp(),
        finished_at: resp.finished_at.map(|t| t.timestamp()),
    }))
}

#[derive(Serialize, Deserialize, Clone, TS)]
#[ts(export, export_to = ".generated/CreateQuizRequest.ts")] 
struct CreateQuizRequest {
    login_token: String,
    id: String,
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
) -> Result<Json<CreateQuizResponse>, Error> {
    let auth_data = super::auth::check_auth(
        &app_state.pool,
        &create_quiz_req.login_token,
    )
    .await
    .map_err(Error::new)?;

    let o_id = uuid::Uuid::from_str(&create_quiz_req.id)
        .map_err(|_| Error::new("Invalid id".to_string()))?;

    let rec = sqlx::query!(
        "SELECT state, guild_id, questions FROM staff_onboardings WHERE id = $1 AND user_id = $2 AND void = false",
        o_id,
        auth_data.user_id
    )
    .fetch_one(&app_state.pool)
    .await
    .map_err(|_| Error::new("Could not find onboarding response".to_string()))?;

    if rec.state != crate::states::OnboardState::InQuiz.to_string() {
        return Err(Error::new(
            "Paradise Protection Protocol is not enabled right now".to_string(),
        ));
    }

    let questions = rec
    .questions
    .unwrap_or(json!({}));

    let quiz_ver = questions
        .get("quiz_ver")
        .unwrap_or(&json!(0))
        .as_i64()
        .unwrap_or(0);

    if quiz_ver == 1 {
        let obj = json!([]);
        let quiz_qvals = questions.get("questions").unwrap_or(&obj).as_array();

        if let Some(question_vals) = quiz_qvals {
            let mut questions = vec![];

            for q in question_vals {
                // Parse question as Question
                let question: PublicQuestion = serde_json::from_value(q.clone()).map_err(|_| {
                    Error::new("Fatal error: Could not parse question".to_string())
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
        "cache_nonce": botox::crypto::gen_random(12)
    });

    sqlx::query!(
        "UPDATE staff_onboardings SET questions = $1 WHERE id = $2",
        quiz,
        o_id
    )
    .execute(&app_state.pool)
    .await
    .map_err(|_| Error::new("Could not save questions".to_string()))?;

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
    login_token: String,
    id: String,
    quiz_answers: HashMap<String, String>,
    sv_code: String,
}

#[axum_macros::debug_handler]
async fn submit_onboarding(
    State(app_state): State<Arc<AppState>>,
    Json(submit_onboarding_req): Json<SubmitOnboarding>,
) -> Result<impl IntoResponse, Error> {
    let auth_data = super::auth::check_auth(
        &app_state.pool,
        &submit_onboarding_req.login_token,
    )
    .await
    .map_err(Error::new)?;

    let o_id = uuid::Uuid::from_str(&submit_onboarding_req.id)
        .map_err(|_| Error::new("Invalid id".to_string()))?;


    let rec = sqlx::query!(
        "SELECT state, guild_id, questions FROM staff_onboardings WHERE id = $1 AND user_id = $2 AND void = false",
        o_id,
        auth_data.user_id
    )
    .fetch_one(&app_state.pool)
    .await
    .map_err(|_| Error::new("Could not find onboarding response".to_string()))?;
    

    if rec.state != crate::states::OnboardState::InQuiz.to_string() {
        return Err(Error::new(
            "Paradise Protection Protocol is not enabled right now".to_string(),
        ));
    }

    // Check onboard_resp with corresponding resp id
    let questions = rec
    .questions
    .unwrap_or(json!({}));

    let quiz_ver = questions
        .get("quiz_ver")
        .unwrap_or(&json!(0))
        .as_i64()
        .unwrap_or(0);

    if quiz_ver != 1 {
        // Corrupt data, reset questions and error
        sqlx::query!(
            "UPDATE staff_onboardings SET questions = $1 WHERE id = $2",
            json!({}),
            o_id,
        )
        .execute(&app_state.pool)
        .await
        .map_err(|_| Error::new("Could not reset questions".to_string()))?;

        return Err(Error::new(
            "Quiz could not be found and hence has been reset, reload the page and try again"
                .to_string(),
        ));
    }

    if !crate::finish::check_code(
        &app_state.pool,
        o_id.hyphenated().to_string().as_str(),
        &auth_data.user_id,
        &submit_onboarding_req.sv_code,
    )
    .await
    .map_err(|e| Error::new(e.to_string()))?
    {
        // Incorrect code
        return Err(Error::new("Incorrect staff verification code".to_string()));
    }

    // Next parse the questions in DB
    let obj = json!([]);
    let quiz_qvals = questions.get("questions").unwrap_or(&obj).as_array();

    let mut questions = vec![];

    if let Some(question_vals) = quiz_qvals {
        for q in question_vals {
            // Parse question as Question
            let question: Question = serde_json::from_value(q.clone()).map_err(|_| {
                Error::new("Fatal error: Could not parse question".to_string())
            })?;

            questions.push(question);
        }
    }

    // Now check that every answer is present, adding them to a vec in the order that they have been found
    for question in &questions {
        let answer = submit_onboarding_req
            .quiz_answers
            .get(&question.question)
            .ok_or(Error::new(
                "Missing answer for ".to_string() + &question.question,
            ))?;

        match question.data {
            QuestionData::Short => {
                if answer.len() < 50 {
                    return Err(Error::new(
                        "Short answer questions must be at least 50 characters long".to_string(),
                    ));
                }
            }
            QuestionData::Long => {
                if answer.len() < 750 {
                    return Err(Error::new(
                        "Long answer questions must be at least 750 characters long".to_string(),
                    ));
                }
            }
            QuestionData::MultipleChoice(ref choices) => {
                if !choices.contains(answer) {
                    return Err(Error::new(
                        "Invalid answer for multiple choice question".to_string(),
                    ));
                }
            }
        }
    }

    sqlx::query!(
        "UPDATE staff_onboardings SET questions = $1, answers = $2, state = $3 WHERE id = $4",
        serde_json::to_value(questions).map_err(|_| {
            Error::new("Fatal error: Could not serialize questions".to_string())
        })?,
        serde_json::to_value(submit_onboarding_req.quiz_answers)
            .map_err(|_| Error::new("Could not serialize answers".to_string()))?,
        crate::states::OnboardState::PendingManagerReview.to_string(),
        o_id
    )
    .execute(&app_state.pool)
    .await
    .map_err(|_| Error::new("Could not save answers".to_string()))?;

    // Send message on discord
    crate::config::CONFIG.channels.onboarding_channel.say(
        &app_state.cache_http,
        format!(
            "User <@{}> has submitted their onboarding quiz. Please see {}/onboarding/resp/{} to review it, then use the ``/admin approve/deny`` commands to approve or deny it.", 
            auth_data.user_id,
            crate::config::CONFIG.panel_url,
            o_id
        )
    ).await.map_err(|_| Error::new("Could not send message on discord".to_string()))?;

    Ok((StatusCode::NO_CONTENT).into_response())
}
