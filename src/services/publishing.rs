use crate::{
    core::{Context, Error, permissions},
    services::{
        content::Document,
        storage::{Publication, template},
    },
};
use anyhow::{Context as _, ensure};
use poise::serenity_prelude as s;
use std::time::Duration;

pub async fn authorize(ctx: Context<'_>, kind: &str, embeds: bool) -> Result<s::ChannelId, Error> {
    let cfg = permissions::guild_config(ctx)?;
    let modules = cfg.modules.unwrap_or(ctx.data().config.global.modules);
    ensure!(
        match kind {
            "rules" => modules.rules,
            "links" => modules.links,
            _ => modules.messaging && cfg.messaging.enabled,
        },
        "Módulo desabilitado"
    );
    let auth = permissions::administrative(ctx, s::Permissions::MANAGE_MESSAGES).await?;
    let id = cfg.channel(kind)?;
    let channel = id
        .to_channel(ctx.http())
        .await?
        .guild()
        .context("Canal inválido")?;
    ensure!(
        matches!(channel.kind, s::ChannelType::Text | s::ChannelType::News),
        "Use canal de texto ou anúncios"
    );
    let mut required = s::Permissions::VIEW_CHANNEL | s::Permissions::SEND_MESSAGES;
    if embeds {
        required |= s::Permissions::EMBED_LINKS;
    }
    permissions::ensure_bot_channel_permissions(&auth, &channel, required)?;
    Ok(id)
}
pub async fn load(ctx: Context<'_>, kind: &str) -> Result<(Document, u64), Error> {
    authorize(ctx, kind, kind != "announcements").await?;
    let cfg = permissions::guild_config(ctx)?;
    let _guard = ctx.data().store.gate.lock().await;
    let state = ctx.data().store.read(cfg.guild_id)?;
    let mut doc = template(&ctx.data().templates, cfg, kind, &state)?;
    doc.entries.sort_by_key(|entry| entry.order);
    Ok((doc, state.revision))
}

/// Prévia vinculada ao usuário, mensagem efêmera e ID único da interação.
pub async fn preview(
    ctx: Context<'_>,
    kind: &str,
    key: String,
    mut doc: Document,
    revision: u64,
    delete: bool,
) -> Result<(), Error> {
    let id = ctx.id();
    let publish = format!("publish:{id}");
    let cancel = format!("cancel:{id}");
    let edit = format!("edit:{id}");
    let save = format!("save:{id}");
    let deadline = tokio::time::Instant::now() + Duration::from_secs(240);
    loop {
        let embeds = doc.embeds(kind)?;
        let mut buttons = vec![
            s::CreateButton::new(&publish)
                .label(if delete {
                    "Confirmar exclusão"
                } else {
                    "Confirmar / Publicar"
                })
                .style(s::ButtonStyle::Primary),
            s::CreateButton::new(&cancel).label("Cancelar"),
        ];
        if !delete && kind == "announcements" {
            buttons.push(s::CreateButton::new(&edit).label("Editar"));
        }
        if !delete && kind != "announcements" {
            buttons.push(s::CreateButton::new(&save).label("Salvar rascunho"));
            if !doc.entries.is_empty() {
                buttons.push(s::CreateButton::new(&edit).label("Editar item"));
            }
        }
        let reply = ctx
            .send(
                poise::CreateReply {
                    embeds,
                    ..Default::default()
                }
                .content(if doc.text.is_empty() {
                    "Prévia privada • sessão de confirmação: 4 minutos"
                } else {
                    &doc.text
                })
                .components(vec![s::CreateActionRow::Buttons(buttons)])
                .ephemeral(true)
                .allowed_mentions(s::CreateAllowedMentions::new()),
            )
            .await?;
        let message = reply.message().await?;
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        let interaction = message
            .await_component_interaction(ctx.serenity_context())
            .author_id(ctx.author().id)
            .timeout(remaining)
            .await;
        let Some(interaction) = interaction else {
            reply
                .edit(
                    ctx,
                    poise::CreateReply::default()
                        .content("Sessão expirada. Execute o comando novamente.")
                        .components(vec![]),
                )
                .await?;
            return Ok(());
        };
        if interaction.data.custom_id == edit {
            let updated = if kind == "announcements" {
                crate::modules::content_commands::announcement_modal(ctx, Some(&interaction), &doc)
                    .await?
            } else {
                crate::modules::content_commands::collection_modal(ctx, &interaction, &doc).await?
            };
            reply
                .edit(ctx, poise::CreateReply::default().components(vec![]))
                .await?;
            if let Some(updated) = updated {
                ensure!(
                    tokio::time::Instant::now() < deadline,
                    "Sessão expirada; execute novamente"
                );
                doc = updated;
                continue;
            }
            return Ok(());
        }
        interaction
            .create_response(ctx.http(), s::CreateInteractionResponse::Acknowledge)
            .await?;
        reply
            .edit(ctx, poise::CreateReply::default().components(vec![]))
            .await?;
        if interaction.data.custom_id == save {
            authorize(ctx, kind, true).await?;
            let guild = permissions::guild_config(ctx)?.guild_id;
            let _guard = ctx.data().store.gate.lock().await;
            let mut state = ctx.data().store.read(guild)?;
            ensure!(
                state.revision == revision && state.pending.is_none(),
                "Estado alterado ou pendente; abra nova prévia"
            );
            state.documents.insert(key.clone(), doc.clone());
            state.revision += 1;
            ctx.data().store.write(guild, &state)?;
            tracing::info!(%guild, %key, actor = %ctx.author().id, "Rascunho salvo");
            ctx.say("Rascunho salvo. Use publish ou update para publicar.")
                .await?;
            return Ok(());
        }
        if interaction.data.custom_id != publish {
            ctx.say("Operação cancelada.").await?;
            return Ok(());
        }
        let result = commit(ctx, kind, &key, &doc, revision, delete).await?;
        ctx.say(result).await?;
        return Ok(());
    }
}

async fn commit(
    ctx: Context<'_>,
    kind: &str,
    key: &str,
    doc: &Document,
    revision: u64,
    delete: bool,
) -> Result<String, Error> {
    let embeds = doc.embeds(kind)?;
    let channel = authorize(ctx, kind, !embeds.is_empty()).await?;
    let guild = permissions::guild_config(ctx)?.guild_id;
    let _guard = ctx.data().store.gate.lock().await;
    let mut state = ctx.data().store.read(guild)?;
    ensure!(
        state.revision == revision,
        "Conteúdo alterado por outra operação. Abra nova prévia."
    );
    ensure!(
        state.pending.is_none(),
        "Existe operação com resultado incerto. Confira o canal e o state.json antes de repetir; publicação bloqueada para evitar duplicação."
    );
    let existing = state.publications.get(key).cloned();
    ensure!(
        kind != "announcements"
            || existing.is_some()
            || key == crate::services::announcements::new_id(ctx.id()),
        "Anúncio já não possui publicação registrada; crie um novo anúncio, não uma edição"
    );
    if kind == "rules" || kind == "links" {
        let action = if existing.is_some() {
            "update"
        } else {
            "publish"
        };
        let guild_config = permissions::guild_config(ctx)?;
        ensure!(
            guild_config.command_enabled(
                &format!("{kind}.{action}"),
                ctx.data().config.global.modules
            ),
            "Publicação/atualização desabilitada nesta guilda"
        );
    }
    ensure!(!delete || existing.is_some(), "Publicação inexistente");
    if let Some(publication) = &existing {
        let auth = permissions::administrative(ctx, s::Permissions::MANAGE_MESSAGES).await?;
        let target = channel
            .to_channel(ctx.http())
            .await?
            .guild()
            .context("Canal inválido")?;
        permissions::ensure_bot_channel_permissions(
            &auth,
            &target,
            s::Permissions::READ_MESSAGE_HISTORY,
        )?;
        ensure!(
            publication.channel == channel,
            "Canal configurado mudou; revise o registro antes de atualizar"
        );
        match channel.message(ctx.http(), publication.message).await {
            Ok(message) => ensure!(
                message.author.id == ctx.serenity_context().cache.current_user().id,
                "Mensagem não pertence ao bot"
            ),
            Err(s::Error::Http(s::HttpError::UnsuccessfulRequest(response)))
                if response.status_code.as_u16() == 404 =>
            {
                state.publications.remove(key);
                if kind == "announcements" {
                    state.documents.remove(key);
                }
                state.revision += 1;
                ctx.data().store.write(guild, &state)?;
                if kind == "announcements" {
                    return Ok(format!(
                        "Mensagem já excluída no Discord. Registro `{key}` reconciliado; nenhuma nova mensagem foi enviada."
                    ));
                }
                anyhow::bail!(
                    "Mensagem excluída manualmente. Registro recuperado; use publish e confirme uma nova publicação."
                );
            }
            Err(error) => return Err(error.into()),
        }
    }
    state.pending = Some(format!("{key}:{}", ctx.id()));
    ctx.data().store.write(guild, &state)?;
    // Em resultado HTTP incerto, mantém journal pendente: nunca repete um envio cegamente.
    if delete {
        let publication = existing.context("Publicação inexistente")?;
        channel
            .delete_message(ctx.http(), publication.message)
            .await?;
        state.publications.remove(key);
        state.documents.remove(key);
    } else {
        let message = if let Some(publication) = existing {
            channel
                .edit_message(
                    ctx.http(),
                    publication.message,
                    s::EditMessage::new()
                        .content(&doc.text)
                        .embeds(embeds)
                        .allowed_mentions(s::CreateAllowedMentions::new()),
                )
                .await?
        } else {
            channel
                .send_message(
                    ctx.http(),
                    s::CreateMessage::new()
                        .content(&doc.text)
                        .embeds(embeds)
                        .allowed_mentions(s::CreateAllowedMentions::new()),
                )
                .await?
        };
        state.publications.insert(
            key.into(),
            Publication {
                channel,
                message: message.id,
            },
        );
        state.documents.insert(key.into(), doc.clone());
    }
    state.pending = None;
    state.revision += 1;
    ctx.data().store.write(guild, &state)?;
    tracing::info!(%guild, %channel, actor = %ctx.author().id, %key, delete, "Publicação gerenciada atualizada");
    Ok(if delete {
        format!("Anúncio `{key}` excluído e registro removido.")
    } else {
        let publication = &state.publications[key];
        format!(
            "Operação concluída. Identificador: `{key}`\nMessage ID: `{}`\n[Ver mensagem]({})",
            publication.message,
            crate::services::announcements::message_link(guild, publication)
        )
    })
}
