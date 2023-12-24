use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone)]
pub struct AuthData {
    pub user_id: String,
    pub created_at: i64,
    pub state: String,
}