use super::{Context, Error, permissions};

pub fn all() -> Vec<poise::Command<super::Data, Error>> {
    use crate::modules;
    vec![
        ping(),
        modules::roles::role(),
        modules::content_commands::rules(),
        modules::content_commands::links(),
        modules::content_commands::announcement(),
        modules::messaging::announce(),
        modules::profile::profile(),
    ]
}
#[derive(Debug, Clone, Copy, Default, poise::ChoiceParameter)]
pub enum PingMode {
    #[default]
    #[name = "simples"]
    Simples,
    #[name = "status"]
    Status,
}
impl PingMode {
    pub fn action(self) -> &'static str {
        match self {
            Self::Simples => "simples",
            Self::Status => "status",
        }
    }
}
/// Verifica conexão; modo status inclui versão e informações operacionais.
#[poise::command(slash_command, guild_only, ephemeral)]
pub async fn ping(
    ctx: Context<'_>,
    #[description = "Modo da consulta (padrão: simples)"] modo: Option<PingMode>,
) -> Result<(), Error> {
    let action = permissions::guild_config(ctx)?.select_action(
        "ping",
        modo.map(|v| v.action()),
        ctx.data().config.global.modules,
    )?;
    if action == "simples" {
        ctx.say("Pong! Bot online.").await?;
    } else {
        ctx.say(format!("{} v{} | conexão: operacional (interação recebida) | uptime: {}s | guildas configuradas: {} | execução: {}/{}", ctx.data().config.global.app_name, env!("CARGO_PKG_VERSION"), ctx.data().started.elapsed().as_secs(), ctx.data().config.guilds.len(), std::env::consts::OS, std::env::consts::ARCH)).await?;
    }
    Ok(())
}
