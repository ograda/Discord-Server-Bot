use super::{Context, Error};

pub fn all() -> Vec<poise::Command<super::Data, Error>> {
    use crate::modules;
    vec![
        ping(),
        status(),
        modules::messaging::announce(),
        modules::roles::role(),
        modules::content_commands::announcements(),
        modules::content_commands::rules(),
        modules::content_commands::links(),
        modules::profile::profile(),
    ]
}

/// Verifica se o bot está online.
#[poise::command(slash_command, guild_only, ephemeral)]
pub async fn ping(ctx: Context<'_>) -> Result<(), Error> {
    ctx.say("Pong! Bot online.").await?;
    Ok(())
}

/// Exibe versão e tempo de execução.
#[poise::command(slash_command, guild_only, ephemeral)]
pub async fn status(ctx: Context<'_>) -> Result<(), Error> {
    ctx.say(format!(
        "{} v{} | operacional | uptime: {}s",
        ctx.data().config.global.app_name,
        env!("CARGO_PKG_VERSION"),
        ctx.data().started.elapsed().as_secs()
    ))
    .await?;
    Ok(())
}
