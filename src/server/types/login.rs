use serenity::all::UserId;
use std::str::FromStr;
use std::fmt::{Display, Formatter};

pub enum ConfirmLoginState {
    JoinOnboardingServer(UserId),
    CreateSession(String),
}

impl FromStr for ConfirmLoginState {
    type Err = crate::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let split = s.split('.').collect::<Vec<&str>>();

        if split.len() != 2 {
            return Err("Invalid state".into());
        }

        match split[0] {
            "create_session" => {
                // Hex decode the second bit
                let decoded = data_encoding::HEXLOWER.decode(split[1].as_bytes())?;

                let decoded_str = String::from_utf8(decoded)?;

                Ok(ConfirmLoginState::CreateSession(decoded_str))
            },
            "jos" => {
                let uid = split[1].parse::<UserId>()?;

                Ok(ConfirmLoginState::JoinOnboardingServer(uid))
            },
            _ => Err("Invalid state".into())
        }
    }
}

impl Display for ConfirmLoginState {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfirmLoginState::JoinOnboardingServer(uid) => write!(f, "jos.{}", uid),
            ConfirmLoginState::CreateSession(redirect_url) => {
                let encoded = data_encoding::HEXLOWER.encode(redirect_url.as_bytes());
                write!(f, "create_session.{}", encoded)
            },
        }
    }
}

impl ConfirmLoginState {
    /// Returns the scopes needed for this state
    pub fn needed_scopes(&self) -> Vec<&str> {
        match self {
            ConfirmLoginState::JoinOnboardingServer(_) => vec!["identify", "guilds.join"],
            ConfirmLoginState::CreateSession(_) => vec!["identify"],
        }
    }

    /// Returns the URL to redirect the user to for login
    pub fn make_login_url(&self, client_id: &str) -> String {
        format!(
            "https://discord.com/api/oauth2/authorize?client_id={}&redirect_uri={}/confirm-login&scope={}&state={}&response_type=code",
            client_id,
            crate::config::CONFIG.persepolis_domain,
            self.needed_scopes().join("%20"),
            self
        )
    }
}
