use super::{Data, Error, commands, permissions};
use crate::config::Config;
use poise::serenity_prelude as s;
use std::time::Instant;

pub async fn run(token: String, config: Config) -> Result<(), Error> {
    let profile_client = crate::services::profile::Client::new()?;
    let store = crate::services::storage::Store::open(
        std::env::var("DATA_DIR")
            .unwrap_or_else(|_| "data".into())
            .into(),
    )?;
    let templates: std::path::PathBuf = std::env::var("TEMPLATE_DIR")
        .unwrap_or_else(|_| "templates".into())
        .into();
    for guild in config.guilds.values() {
        let state = store.read(guild.guild_id)?;
        for kind in ["announcements", "rules", "links"] {
            crate::services::storage::template(&templates, guild, kind, &state)?;
        }
    }
    let framework = poise::Framework::builder()
        .options(poise::FrameworkOptions {
            commands: commands::all(),
            skip_checks_for_owners: false,
            allowed_mentions: Some(s::CreateAllowedMentions::new()),
            command_check: Some(|ctx| {
                Box::pin(async move {
                    let guild = permissions::guild_config(ctx)?;
                    let path = ctx.command().qualified_name.replace(' ', ".");
                    if !guild.command_enabled(&path, ctx.data().config.global.modules) {
                        ctx.send(poise::CreateReply::default().content("Comando desabilitado nesta guilda.").ephemeral(true)).await?;
                        return Ok(false);
                    }
                    Ok(true)
                })
            }),
            on_error: |error| {
                Box::pin(async move {
                    match error {
                        poise::FrameworkError::Setup { error, .. } => {
                            tracing::error!(
                                error = ?error,
                                "Falha na inicialização do framework"
                            );
                        }

                        poise::FrameworkError::Command { ctx, error, .. } => {
                            tracing::warn!(command = ctx.command().name, "Falha em comando administrativo");
                            let message = if error.downcast_ref::<s::Error>().is_some() {
                                "Falha na API Discord. Verifique as permissões. Se houve envio com resultado incerto, consulte o registro pending antes de repetir.".to_string()
                            } else if error.downcast_ref::<std::io::Error>().is_some() || error.downcast_ref::<serde_json::Error>().is_some() {
                                "Falha no armazenamento. Verifique o volume de dados e restaure um backup se necessário; não repita publicações antes de conferir o estado.".to_string()
                            } else { error.to_string() };
                            let _ = ctx.send(poise::CreateReply::default().content(message).ephemeral(true)).await;
                        }

                        other => {
                            if let Err(error) = poise::builtins::on_error(other).await {
                                tracing::error!(
                                    error = ?error,
                                    "Falha no tratamento de erro do Poise"
                                );
                            }
                        }
                    }
                })
            },
            event_handler: |_, event, _, _| {
                Box::pin(async move {
                    match event {
                        s::FullEvent::Ready { data_about_bot } => {
                            tracing::info!(bot = %data_about_bot.user.id, "Conectado ao Discord")
                        }
                        s::FullEvent::Resume { .. } => tracing::info!("Sessão Discord retomada"),
                        _ => {}
                    }
                    Ok(())
                })
            },
            ..Default::default()
        })
        .setup(move |ctx, _, _framework| {
            Box::pin(async move {
                for (id, guild) in &config.guilds {
                    let modules = guild.modules.unwrap_or(config.global.modules);
                    let registered: Vec<_> = crate::config::availability::COMMANDS.iter().filter(|name| guild.command_enabled(name, config.global.modules)).copied().collect();
                    tracing::info!(alias = guild.alias.as_deref().unwrap_or("sem-alias"), guild = %id, ?modules, commands = ?registered, "Preparando comandos por guilda");
                    for name in crate::config::availability::COMMANDS {
                        if let Some(reason) = crate::config::availability::omission_reason(guild, name, config.global.modules) {
                            tracing::info!(alias = guild.alias.as_deref().unwrap_or("sem-alias"), guild = %id, command = name, reason, "Comando omitido");
                        }
                    }
                    let commands = crate::config::availability::registered(guild, config.global.modules);
                    if let Err(error) = poise::builtins::register_in_guild(ctx, &commands, *id).await {
                        tracing::error!(guild = %id, "Falha no registro remoto de comandos");
                        if guild.enabled { return Err(error.into()); }
                        // A disabled guild may no longer have the bot installed.
                        // Runtime checks still reject all its interactions.
                        tracing::warn!(guild = %id, "Não foi possível limpar comandos da guilda desabilitada");
                        continue;
                    }
                    tracing::info!(guild = %id, "Comandos registrados");
                }
                Ok(Data {
                    profile_client,
                    store,
                    templates,
                    config,
                    started: Instant::now(),
                })
            })
        })
        .build();
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
