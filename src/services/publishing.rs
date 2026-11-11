use crate::{
    core::{Context, Error, permissions},
    services::{content::Document, storage::Publication, ui},
};
use anyhow::{Context as _, ensure};
use poise::serenity_prelude as s;

pub async fn authorize_channel(
    ctx: Context<'_>,
    kind: &str,
    channel: s::ChannelId,
    embeds: bool,
) -> Result<s::GuildChannel, Error> {
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
    let target = channel
        .to_channel(ctx.http())
        .await?
        .guild()
        .context("Canal inválido")?;
    ensure!(
        matches!(target.kind, s::ChannelType::Text | s::ChannelType::News),
        "Use canal de texto ou anúncios"
    );
    let mut required = s::Permissions::VIEW_CHANNEL | s::Permissions::SEND_MESSAGES;
    if embeds {
        required |= s::Permissions::EMBED_LINKS;
    }
    permissions::ensure_bot_channel_permissions(&auth, &target, required)?;
    Ok(target)
}
pub async fn authorize(ctx: Context<'_>, kind: &str, embeds: bool) -> Result<s::ChannelId, Error> {
    let id = permissions::guild_config(ctx)?.channel(kind)?;
    authorize_channel(ctx, kind, id, embeds).await?;
    Ok(id)
}
pub async fn load(ctx: Context<'_>, kind: &str) -> Result<(Document, u64), Error> {
    permissions::administrative(ctx, s::Permissions::MANAGE_MESSAGES).await?;
    let cfg = permissions::guild_config(ctx)?;
    let _guard = ctx.data().store.gate.lock().await;
    let state = ctx.data().store.read(cfg.guild_id)?;
    let doc = if kind == "rules" {
        ctx.data().catalog.snapshot(cfg.guild_id).await?.rules
    } else {
        super::storage::template(&ctx.data().templates, cfg, kind, &state)?
    };
    Ok((doc, state.revision))
}
pub struct Plan {
    pub operation: u64,
    pub kind: &'static str,
    pub action: String,
    pub key: String,
    pub document: Document,
    pub channel: s::ChannelId,
    pub revision: u64,
    pub content_revision: Option<u64>,
    pub delete: bool,
}
pub async fn preview(ctx: Context<'_>, plan: Plan) -> Result<(), Error> {
    let label = if plan.delete { "Remover" } else { "Publicar" };
    if !ui::confirm(
        ctx,
        "Prévia privada; confirme para modificar a publicação.",
        label,
        Some((&plan.document, plan.kind)),
    )
    .await?
    {
        return Ok(());
    }
    let result = commit(ctx, &plan).await?;
    ctx.say(result).await?;
    Ok(())
}
async fn commit(ctx: Context<'_>, plan: &Plan) -> Result<String, Error> {
    let cfg = permissions::guild_config(ctx)?;
    let guild = cfg.guild_id;
    permissions::action(ctx, plan.kind_to_root(), &plan.action)?;
    let target = authorize_channel(
        ctx,
        plan.kind,
        plan.channel,
        !plan.document.embeds(plan.kind)?.is_empty(),
    )
    .await?;
    let has_publication = {
        let _guard = ctx.data().store.gate.lock().await;
        ctx.data()
            .store
            .read(guild)?
            .publications
            .contains_key(&plan.key)
    };
    if has_publication {
        let auth = permissions::administrative(ctx, s::Permissions::MANAGE_MESSAGES).await?;
        permissions::ensure_bot_channel_permissions(
            &auth,
            &target,
            s::Permissions::READ_MESSAGE_HISTORY,
        )?;
    }
    let bot = ctx.serenity_context().cache.current_user().id;
    execute(
        &ctx.data().store,
        &ctx.data().catalog,
        guild,
        bot,
        ctx.http(),
        plan,
    )
    .await
}

/// Transaction core, independent of interactions; callers authorize the operation and channel first.
pub async fn execute(
    store: &super::storage::Store,
    catalog: &super::catalog::Catalog,
    guild: s::GuildId,
    bot: s::UserId,
    http: &s::Http,
    plan: &Plan,
) -> Result<String, Error> {
    let embeds = plan.document.embeds(plan.kind)?;
    let _guard = store.gate.lock().await;
    let mut state = store.read(guild)?;
    ensure!(
        state.revision == plan.revision,
        "Estado alterado por outra operação; abra uma nova prévia"
    );
    ensure!(
        state.pending.is_none(),
        "Operação pendente com resultado incerto; confira o canal e o estado antes de repetir"
    );
    if let Some(revision) = plan.content_revision {
        ensure!(
            catalog.snapshot(guild).await?.revision == revision,
            "TOML em memória mudou; abra uma nova prévia"
        );
    }
    let existing = state.publications.get(&plan.key).cloned();
    ensure!(
        !plan.delete || existing.is_some(),
        "Publicação não encontrada nesta guilda"
    );
    ensure!(
        plan.kind != "announcements"
            || existing.is_some()
            || (plan.action == "create"
                && plan.key == super::announcements::new_id(plan.operation)),
        "Anúncio sem publicação registrada; crie um novo anúncio"
    );
    let mut missing = false;
    if let Some(publication) = &existing {
        ensure!(
            publication.channel == plan.channel,
            "Canal configurado mudou; referências preservadas, reconcilie o registro antes de continuar"
        );
        match plan.channel.message(http, publication.message).await {
            Ok(message) => ensure!(
                message.author.id == bot,
                "Mensagem registrada não pertence ao bot"
            ),
            Err(s::Error::Http(s::HttpError::UnsuccessfulRequest(response)))
                if response.status_code.as_u16() == 404 =>
            {
                ensure!(
                    plan.delete,
                    "Mensagem original não encontrada (404). IDs preservados; use Remove para reconciliar antes de uma nova publicação"
                );
                missing = true;
            }
            Err(error) => return Err(error.into()),
        }
    }
    state.pending = Some(format!("{}:{}", plan.key, plan.operation));
    store.write(guild, &state)?;
    tracing::info!(%guild, module = plan.kind, action = plan.action, stage = "http", channel = %plan.channel, "Aplicando publicação confirmada");
    // Keep pending on uncertain HTTP outcomes. A second click cannot repeat this transaction.
    if plan.delete {
        let publication = existing.context("Publicação inexistente")?;
        if !missing {
            match plan.channel.delete_message(http, publication.message).await {
                Ok(()) => (),
                Err(s::Error::Http(s::HttpError::UnsuccessfulRequest(response)))
                    if response.status_code.as_u16() == 404 => {}
                Err(error) => return Err(error.into()),
            }
        }
        state.publications.remove(&plan.key);
        if plan.kind == "announcements" {
            state.documents.remove(&plan.key);
        }
    } else {
        let message = if let Some(publication) = existing {
            plan.channel
                .edit_message(
                    http,
                    publication.message,
                    s::EditMessage::new()
                        .content(&plan.document.text)
                        .embeds(embeds)
                        .allowed_mentions(s::CreateAllowedMentions::new()),
                )
                .await?
        } else {
            plan.channel
                .send_message(
                    http,
                    s::CreateMessage::new()
                        .content(&plan.document.text)
                        .embeds(embeds)
                        .allowed_mentions(s::CreateAllowedMentions::new()),
                )
                .await?
        };
        state.publications.insert(
            plan.key.clone(),
            Publication {
                channel: plan.channel,
                message: message.id,
            },
        );
        if plan.kind == "announcements" {
            state
                .documents
                .insert(plan.key.clone(), plan.document.clone());
        }
    }
    state.pending = None;
    state.revision += 1;
    store.write(guild, &state)?;
    if plan.delete {
        Ok(format!(
            "Publicação `{}` removida; definição TOML preservada.",
            plan.key
        ))
    } else {
        let publication = &state.publications[&plan.key];
        Ok(format!(
            "Conteúdo publicado; estado salvo nesta guilda. Identificador: `{}`\nMessage ID: `{}`\n[Ver mensagem]({})",
            plan.key,
            publication.message,
            super::announcements::message_link(guild, publication)
        ))
    }
}
impl Plan {
    fn kind_to_root(&self) -> &'static str {
        if self.kind == "announcements" {
            "announcement"
        } else {
            self.kind
        }
    }
}
