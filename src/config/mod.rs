pub mod availability;
mod loader;
pub mod migration;
pub mod registry;
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
    #[serde(default)]
    pub guilds: Option<HashMap<String, GuildRegistration>>,
}

#[derive(Debug, serde::Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GuildRegistration {
    pub id: GuildId,
    pub name: String,
    #[serde(default = "enabled_default")]
    pub enabled: bool,
}
fn enabled_default() -> bool {
    true
}

#[derive(Debug, Clone, Copy, serde::Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Modules {
    pub administration: bool,
    pub messaging: bool,
    pub roles: bool,
    #[serde(default)]
    pub rules: bool,
    #[serde(default)]
    pub links: bool,
    #[serde(default)]
    pub profile: bool,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GuildConfig {
    pub guild_id: GuildId,
    #[serde(default)]
    pub profile: crate::services::profile::ProfileConfig,
    pub alias: Option<String>,
    pub name: Option<String>,
    #[serde(default = "enabled_default")]
    pub enabled: bool,
    #[serde(skip)]
    pub commands: HashMap<String, bool>,
    #[serde(skip)]
    pub contents: HashMap<String, crate::services::content::Document>,
    #[serde(default)]
    pub templates: HashMap<String, String>,
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
    pub rules: Option<ChannelId>,
    pub links: Option<ChannelId>,
    #[serde(default)]
    pub custom: HashMap<String, ChannelId>,
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
        self.profile.validate()?;
        let modules = self.modules.unwrap_or(defaults);
        self.validate_registry()?;
        ensure!(
            !modules.rules || self.channels.rules.is_some(),
            "rules exige channels.rules"
        );
        ensure!(
            !modules.links || self.channels.links.is_some(),
            "links exige channels.links"
        );
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
