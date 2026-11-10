use super::{Data, Error, commands, permissions};
use crate::{config::Config, modules};
use poise::serenity_prelude as s;
use std::time::Instant;

pub async fn run(token: String, config: Config) -> Result<(), Error> {
    let framework = poise::Framework::builder()
        .options(poise::FrameworkOptions {
            commands: vec![commands::ping(), commands::status(), modules::messaging::announce(), modules::roles::role()],
            skip_checks_for_owners: false,
            allowed_mentions: Some(s::CreateAllowedMentions::new()),
            command_check: Some(|ctx| Box::pin(async move { permissions::guild_config(ctx)?; Ok(true) })),
            on_error: |error| Box::pin(async move {
                // Não registra argumentos, conteúdo de mensagens ou respostas brutas da API.
                match error {
                    poise::FrameworkError::Command { error, ctx, .. } => {
                        tracing::warn!(command = ctx.command().name, "Falha ao executar comando");
                        let message = if error.downcast_ref::<s::Error>().is_some() {
                            "Falha na API Discord. Verifique permissões e tente novamente.".to_owned()
                        } else { error.to_string() };
                        let _ = ctx.send(poise::CreateReply::default().content(message).ephemeral(true)).await;
                    }
                    poise::FrameworkError::CommandCheckFailed { ctx, .. } => {
                        let _ = ctx.send(poise::CreateReply::default().content("Servidor não autorizado.").ephemeral(true)).await;
                    }
                    other => {
                        tracing::warn!(kind = ?std::mem::discriminant(&other), "Erro do framework");
                        if let Some(ctx) = other.ctx() {
                            let _ = ctx.send(poise::CreateReply::default().content("Não foi possível executar o comando. Verifique parâmetros e permissões.").ephemeral(true)).await;
                        }
                    }
                }
            }),
            event_handler: |_, event, _, _| Box::pin(async move {
                match event {
                    s::FullEvent::Ready { data_about_bot } => tracing::info!(bot = %data_about_bot.user.id, "Conectado ao Discord"),
                    s::FullEvent::Resume { .. } => tracing::info!("Sessão Discord retomada"),
                    _ => {}
                }
                Ok(())
            }),
            ..Default::default()
        })
        .setup(move |ctx, _, framework| Box::pin(async move {
            for id in config.guilds.keys() {
                poise::builtins::register_in_guild(ctx, &framework.options().commands, *id).await?;
                tracing::info!(guild = %id, "Comandos registrados");
            }
            Ok(Data { config, started: Instant::now() })
        })).build();
    let mut client = s::ClientBuilder::new(token, s::GatewayIntents::GUILDS)
        .framework(framework)
        .await?;
    let manager = client.shard_manager.clone();
    tokio::select! {
        result = client.start() => result?,
        result = shutdown() => { result?; tracing::info!("Encerrando conexões"); manager.shutdown_all().await; }
    }
    tracing::info!("Bot encerrado");
    Ok(())
}
async fn shutdown() -> std::io::Result<()> {
    #[cfg(unix)]
    {
        let mut term = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
        tokio::select! { result = tokio::signal::ctrl_c() => result, _ = term.recv() => Ok(()) }
    }
    #[cfg(not(unix))]
    tokio::signal::ctrl_c().await
}
