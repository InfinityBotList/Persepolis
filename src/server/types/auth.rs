use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Serialize, Deserialize, Clone, TS)]
#[ts(export, export_to = ".generated/AuthData.ts")]
pub struct AuthData {
    pub user_id: String,
    pub created_at: i64,
    pub state: String,
}

#[derive(Serialize, Deserialize, Clone, TS)]
#[ts(export, export_to = ".generated/GetAuthData.ts")]
pub struct GetAuthData {
    pub login_token: String,
}

#[derive(Serialize, Deserialize, Clone, TS)]
#[ts(export, export_to = ".generated/CreateLogin.ts")]
pub struct CreateLogin {
    pub state: String,
}