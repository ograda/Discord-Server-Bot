mod loader;
pub use loader::load;

use anyhow::{Result, ensure};
use poise::serenity_prelude::{ChannelId, GuildId, RoleId};
use serde::Deserialize;
use std::collections::HashMap;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GlobalConfig {
    pub app_name: String,
    pub log_level: String,
    pub modules: Modules,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Modules {
    pub administration: bool,
    pub messaging: bool,
    pub roles: bool,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GuildConfig {
    pub guild_id: GuildId,
    pub modules: Option<Modules>,
    #[serde(default)]
    pub channels: Channels,
    #[serde(default)]
    pub roles: Roles,
    #[serde(default)]
    pub messaging: Messaging,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Channels {
    pub announcements: Option<ChannelId>,
    pub moderation_logs: Option<ChannelId>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Roles {
    #[serde(default)]
    pub moderators: Vec<RoleId>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Messaging {
    pub enabled: bool,
}
impl Default for Messaging {
    fn default() -> Self {
        Self { enabled: true }
    }
}

#[derive(Debug)]
pub struct Config {
    pub global: GlobalConfig,
    pub guilds: HashMap<GuildId, GuildConfig>,
}

impl GlobalConfig {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            !self.app_name.trim().is_empty(),
            "app_name não pode ser vazio"
        );
        ensure!(
            ["trace", "debug", "info", "warn", "error", "off"].contains(&self.log_level.as_str()),
            "log_level inválido"
        );
        Ok(())
    }
}
impl GuildConfig {
    pub fn validate(&self, defaults: Modules) -> Result<()> {
        let modules = self.modules.unwrap_or(defaults);
        ensure!(
            !(modules.messaging && self.messaging.enabled) || self.channels.announcements.is_some(),
            "messaging exige channels.announcements"
        );
        let mut ids = std::collections::HashSet::new();
        ensure!(
            self.roles.moderators.iter().all(|id| ids.insert(id)),
            "cargo moderador duplicado"
        );
        Ok(())
    }
}
