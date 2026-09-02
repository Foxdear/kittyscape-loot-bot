use std::env;
use serenity::all::RoleId;
use serenity::model::id::ChannelId;
use serenity::prelude::TypeMapKey;

#[derive(Clone)]
pub struct Config {
    pub mod_channel_id: ChannelId,
    pub log_channel_id: ChannelId,
    pub runelite_channel_id: Option<ChannelId>,
    pub rank_request_channel_id: Option<ChannelId>,
    pub dink_webhook_token: String,
}

impl Config {
    pub fn from_env() -> Result<Self, env::VarError> {
        let mod_channel_id = env::var("MOD_CHANNEL_ID")?
            .parse::<u64>()
            .map_err(|_| env::VarError::NotPresent)?;
            
        // Try to get the log channel ID, fall back to using mod channel if not available
        let log_channel_id = match env::var("BOT_LOG_CHANNEL_ID") {
            Ok(id) => id.parse::<u64>().unwrap_or(mod_channel_id),
            Err(_) => mod_channel_id
        };

        // Optional RuneLite channel ID
        let runelite_channel_id = match env::var("RUNELITE_CHANNEL_ID") {
            Ok(id) => match id.parse::<u64>() {
                Ok(id) => Some(ChannelId::new(id)),
                Err(_) => None
            },
            Err(_) => None
        };

        // Optional Rank Request channel ID
        let rank_request_channel_id = match env::var("RANK_REQUEST_CHANNEL_ID") {
            Ok(id) => match id.parse::<u64>() {
                Ok(id) => Some(ChannelId::new(id)),
                Err(_) => None
            },
            Err(_) => None
        };

        // Shared secret baked into the /dink/{token} webhook path, since Dink can't send
        // custom headers - this is the only thing gating that endpoint from the open internet
        let dink_webhook_token = env::var("DINK_WEBHOOK_TOKEN")?;

        Ok(Self {
            mod_channel_id: ChannelId::new(mod_channel_id),
            log_channel_id: ChannelId::new(log_channel_id),
            runelite_channel_id,
            rank_request_channel_id,
            dink_webhook_token,
        })
    }
}

pub struct ConfigKey;

impl TypeMapKey for ConfigKey {
    type Value = Config;
} 