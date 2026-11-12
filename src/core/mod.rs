pub mod bot;
pub mod commands;
pub mod diagnostics;
pub mod interactions;
pub mod permissions;

use crate::config::Config;
use std::time::Instant;
pub type Error = anyhow::Error;
pub type Context<'a> = poise::Context<'a, Data, Error>;
pub struct Data {
    pub interactions: interactions::Responses,
    pub role_locks: crate::services::roles::RoleLocks,
    pub catalog: crate::services::catalog::Catalog,
    pub profile_client: crate::services::profile::Client,
    pub store: crate::services::storage::Store,
    pub templates: std::path::PathBuf,
    pub config: Config,
    pub started: Instant,
}
