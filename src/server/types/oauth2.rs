use serde::Deserialize;

#[derive(Deserialize)]
pub struct AccessTokenResponse {
    pub access_token: String,
    pub scope: String,
}

#[derive(Deserialize)]
pub struct ConfirmLogin {
    pub code: String,
    pub state: String,
}
