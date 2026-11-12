use super::{Data, Error, commands, permissions};
use crate::config::Config;
use poise::serenity_prelude as s;
use std::time::Instant;

pub async fn run(token: String, config: Config) -> Result<(), Error> {
    let intents = config.global.gateway_intents();
    tracing::info!(
        guild_members = config.global.intents.guild_members,
        "Intents de Gateway configuradas; operações em massa exigem habilitação no Developer Portal"
    );
    let profile_client = crate::services::profile::Client::new()?;
    let store = crate::services::storage::Store::open(
        std::env::var("DATA_DIR")
            .unwrap_or_else(|_| "data".into())
            .into(),
    )?;
    tracing::info!(
        process_id = std::process::id(),
        "Lock exclusivo do diretório de dados adquirido; confira o PID ao investigar instâncias simultâneas"
    );
    let templates: std::path::PathBuf = std::env::var("TEMPLATE_DIR")
        .unwrap_or_else(|_| "templates".into())
        .into();
    let catalog = crate::services::catalog::Catalog::new(&config);
    for guild in config.guilds.values() {
        store.migrate_content(
            guild.guild_id,
            guild
                .link_boards
                .as_deref()
                .and_then(|boards| boards.first()),
        )?;
        let state = store.read(guild.guild_id)?;
        for kind in ["announcements", "rules", "links"] {
            crate::services::storage::template(&templates, guild, kind, &state)?;
            tracing::info!(guild = %guild.guild_id, kind, source = %crate::services::storage::content_source(guild, kind, &state), "Fonte efetiva de conteúdo na inicialização");
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
                            let message = super::diagnostics::failure(ctx, &error);
                            if ctx.command().name == "profile" || error.is::<super::interactions::Failure>() {
                                if super::interactions::may_report(ctx, &error)
                                    && let Err(reply_error) = super::interactions::notify_failure(ctx, message).await {
                                    super::diagnostics::failure(ctx, &reply_error);
                                }
                            } else {
                                let _ = ctx.send(poise::CreateReply::default().content(message).ephemeral(true)).await;
                            }
                        }

                        poise::FrameworkError::ArgumentParse { ctx, .. } => {
                            let error = if ctx.command().name == "role" || ctx.command().name == "role-all" {
                                anyhow::anyhow!("Falha de validação dos parâmetros de cargos. Selecione ação e cargo nos campos nativos; /role também exige um usuário no seletor USER. Não use texto ou all para identificar usuários. Reabra o comando após o registro atualizado.")
                            } else { anyhow::anyhow!("Falha de validação dos parâmetros. Use as escolhas da ação e, para avatar, o campo imagem do tipo Attachment. Se a interface estiver antiga, reabra o comando após o registro atualizado.") };
                            let message = super::diagnostics::failure(ctx, &error);
                            let _ = ctx.send(poise::CreateReply::default().content(message).ephemeral(true)).await;
                        }
                        poise::FrameworkError::CommandStructureMismatch { ctx, description, .. } => {
                            tracing::warn!(guild = ?ctx.interaction.guild_id, module = ctx.command.name, stage = "slash_schema", cause = description, "Registro slash incompatível com os parâmetros recebidos");
                            let _ = poise::Context::Application(ctx).send(poise::CreateReply::default().content("O registro deste comando está desatualizado. Reinicie o bot atualizado para registrar os comandos da guilda e reabra o comando com os tipos e escolhas atuais. Usuário e cargo usam os seletores nativos; avatar utiliza Attachment.").ephemeral(true)).await;
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
            event_handler: |ctx, event, _, data| {
                Box::pin(async move {
                    match event {
                        s::FullEvent::Ready { data_about_bot } => {
                            tracing::info!(bot = %data_about_bot.user.id, "Conectado ao Discord")
                        }
                        s::FullEvent::Resume { .. } => tracing::info!("Sessão Discord retomada"),
                        s::FullEvent::InteractionCreate { interaction: s::Interaction::Component(component) } if crate::services::links::handles_invite_component(&component.data.custom_id) => {
                            crate::services::links::handle_copy(ctx, component, data).await?;
                        }
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
                    let commands = crate::config::availability::remote_commands(guild, config.global.modules);
                    if let Err(error) = id.set_commands(&ctx.http, commands).await {
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
                    interactions: Default::default(),
                    role_locks: Default::default(),
                    catalog,
                    profile_client,
                    store,
                    templates,
                    config,
                    started: Instant::now(),
                })
            })
        })
        .build();
    let mut client = s::ClientBuilder::new(token, intents)
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
