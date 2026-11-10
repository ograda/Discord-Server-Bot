use anyhow::{Context, Result, ensure};
use discord_server_bot::{config, core};
use std::path::Path;

#[tokio::main]
async fn main() -> Result<()> {
    match dotenvy::dotenv() {
        Ok(_) => {}
        Err(e) if e.not_found() => {}
        Err(_) => anyhow::bail!("Não foi possível carregar .env; verifique a sintaxe"),
    }
    let token =
        std::env::var("DISCORD_TOKEN").context("Defina DISCORD_TOKEN no ambiente ou .env")?;
    ensure!(
        !token.trim().is_empty() && token != "replace_with_your_bot_token",
        "Defina um DISCORD_TOKEN válido"
    );
    let root = std::env::var("CONFIG_DIR").unwrap_or_else(|_| "config".into());
    let config = config::load(Path::new(&root))?;
    let filter = std::env::var("RUST_LOG").unwrap_or_else(|_| config.global.log_level.clone());
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::try_new(filter)?)
        .init();
    tracing::info!(app = %config.global.app_name, version = env!("CARGO_PKG_VERSION"), guilds = config.guilds.len(), "Iniciando bot");
    core::bot::run(token, config).await
}
