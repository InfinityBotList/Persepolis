use std::{sync::Arc, num::NonZeroU64};

use axum::{Router, routing::get, extract::{State, Path}, response::{Redirect, IntoResponse, Response}, http::{StatusCode}};
use log::info;
use sqlx::PgPool;

use crate::{cache::CacheHttpImpl, config};

pub struct AppState {
    pub cache_http: CacheHttpImpl,
    pub pool: PgPool,
}

pub async fn setup_server(pool: PgPool, cache_http: CacheHttpImpl) {
    let shared_state = Arc::new(AppState { pool, cache_http });

    let app = Router::new()
        .route("/:uid", get(create_login))
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
    State(state): State<Arc<AppState>>,
    Path(uid): Path<NonZeroU64>,
) -> Redirect {
    // Redirect user to the login page
    let url = format!("https://discord.com/api/oauth2/authorize?client_id={}&redirect_uri={}/confirm-login&scopes={}&state={}", state.cache_http.cache.current_user().id, config::CONFIG.persepolis_domain, "identify", uid);

    Redirect::temporary(&url)
}