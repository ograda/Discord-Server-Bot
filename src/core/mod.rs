pub mod bot;
pub mod commands;
pub mod permissions;

use crate::config::Config;
use std::time::Instant;
pub type Error = anyhow::Error;
pub type Context<'a> = poise::Context<'a, Data, Error>;
pub struct Data {
    pub config: Config,
    pub started: Instant,
}
