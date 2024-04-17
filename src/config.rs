use crate::Error;
use once_cell::sync::Lazy;
use poise::serenity_prelude::{UserId, GuildId, RoleId, ChannelId};
use serde::{Deserialize, Serialize};
use std::{fs::File, io::Write};
use ts_rs::TS;

/// Global config object
pub static CONFIG: Lazy<Config> = Lazy::new(|| Config::load().expect("Failed to load config"));

#[derive(Serialize, Deserialize)]
pub struct Servers {
    pub main: GuildId,
    pub staff: GuildId,
}

impl Default for Servers {
    fn default() -> Self {
        Self {
            main: GuildId::new(758641373074423808),
            staff: GuildId::new(870950609291972618),
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct Roles {
    pub awaiting_staff: RoleId,
    pub main_server_web_moderator: RoleId,
}

impl Default for Roles {
    fn default() -> Self {
        Self {
            awaiting_staff: RoleId::new(1029058929361174678),
            main_server_web_moderator: RoleId::new(1227728825836769320)
        }
    }
}

#[derive(Serialize, Deserialize, Clone, TS)]
#[ts(export, export_to = ".generated/QuestionData.ts")]
pub enum QuestionData {
    #[serde(rename = "short")]
    Short,
    #[serde(rename = "long")]
    Long,
    #[serde(rename = "multiple_choice")]
    MultipleChoice(Vec<String>),
}

#[derive(Serialize, Deserialize, Clone, TS)]
#[ts(export, export_to = ".generated/Question.ts")]
pub struct Question {
    pub question: String,
    pub data: QuestionData,
    pub guidance: Option<String>, // Marking scheme of the question
    pub pinned: bool, // Whether or not the question should be pinned/always present in the quiz
}

#[derive(Serialize, Deserialize)]
pub struct Channels {
    /// Where onboardings are sent to for staff managers to moderate
    pub onboarding_channel: ChannelId,
}

impl Default for Channels {
    fn default() -> Self {
        Self {
            onboarding_channel: ChannelId::new(1087052316533858425),
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct Config {
    pub database_url: String,
    pub client_secret: String,
    pub token: String,
    pub servers: Servers,
    pub roles: Roles,
    pub channels: Channels,
    pub test_bot: UserId,
    pub frontend_url: String,
    pub panel_url: String,
    pub proxy_url: String,
    pub persepolis_domain: String,
    pub questions: Vec<Question>,
    pub testing_server: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            database_url: String::from(""),
            token: String::from(""),
            client_secret: String::from(""),
            servers: Servers::default(),
            roles: Roles::default(),
            channels: Channels::default(),
            test_bot: UserId::new(990885577979224104),
            frontend_url: String::from("https://infinitybots.gg"),
            panel_url: String::from("https://panel.infinitybots.gg"),
            proxy_url: String::from("http://127.0.0.1:3219"),
            persepolis_domain: String::from("https://persepolis.infinitybots.gg"),
            questions: vec![],
            testing_server: "https://discord.com/channels/758641373074423808/815376699868446801/1119292578307776622".to_string()
        }
    }
}

impl Config {
    pub fn load() -> Result<Self, Error> {
        // Delete config.yaml.sample if it exists
        if std::path::Path::new("config.yaml.sample").exists() {
            std::fs::remove_file("config.yaml.sample")?;
        }

        // Create config.yaml.sample
        let mut sample = File::create("config.yaml.sample")?;

        // Write default config to config.yaml.sample
        sample.write_all(serde_yaml::to_string(&Config::default())?.as_bytes())?;

        // Open config.yaml
        let file = File::open("config.yaml");

        match file {
            Ok(file) => {
                // Parse config.yaml
                let cfg: Config = serde_yaml::from_reader(file)?;

                // Return config
                Ok(cfg)
            }
            Err(e) => {
                // Print error
                println!("config.yaml could not be loaded: {}", e);

                // Exit
                std::process::exit(1);
            }
        }
    }
}
