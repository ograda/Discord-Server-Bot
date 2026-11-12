use super::announcements::media::{self, ImageChange};
use crate::{
    core::{Context, Error, permissions},
    services::{content::Document, storage::Publication, ui},
};
use anyhow::{Context as _, ensure};
use poise::serenity_prelude as s;

pub struct Request<'a> {
    pub plan: &'a Plan,
    pub image: &'a ImageChange,
}

pub async fn authorize_attachment(ctx: Context<'_>, target: &s::GuildChannel) -> Result<(), Error> {
    let auth = permissions::administrative(ctx, s::Permissions::MANAGE_MESSAGES).await?;
    permissions::ensure_bot_channel_permissions(&auth, target, s::Permissions::ATTACH_FILES)
}

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
    preview_with_image(ctx, plan, ImageChange::Keep).await
}
pub async fn preview_with_image(
    ctx: Context<'_>,
    plan: Plan,
    image: ImageChange,
) -> Result<(), Error> {
    let mut preview = plan.document.clone();
    if plan.kind == "announcements" {
        image.apply(&mut preview)?;
        if plan.delete {
            preview.image.clear();
        }
        if !plan.delete
            && matches!(image, ImageChange::Keep)
            && media::attachment_filename(&preview.image).is_some()
        {
            let guild = permissions::guild_config(ctx)?.guild_id;
            let state = {
                let _guard = ctx.data().store.gate.lock().await;
                ctx.data().store.read(guild)?
            };
            let publication = state
                .publications
                .get(&plan.key)
                .context("Anexo sem publicação registrada")?;
            ensure!(
                publication.channel == plan.channel,
                "Canal registrado difere do canal configurado"
            );
            let original = publication
                .channel
                .message(ctx.http(), publication.message)
                .await?;
            ensure!(
                original.author.id == ctx.serenity_context().cache.current_user().id
                    && original.guild_id.is_none_or(|id| id == guild),
                "Mensagem original não pertence ao bot"
            );
            media::preview_image(&mut preview, &original, &state, &plan.key)?;
        }
        preview = super::announcements::build(
            ctx.http(),
            permissions::guild_config(ctx)?.guild_id,
            &preview,
        )
        .await?;
    }
    let label = if plan.delete { "Remover" } else { "Publicar" };
    if !ui::confirm_with_files(
        ctx,
        "Prévia privada; confirme para modificar a publicação.",
        label,
        Some((&preview, plan.kind)),
        image.files(),
    )
    .await?
    {
        return Ok(());
    }
    let result = commit_request(
        ctx,
        Request {
            plan: &plan,
            image: &image,
        },
    )
    .await?;
    ctx.say(result).await?;
    Ok(())
}
pub(crate) async fn commit(ctx: Context<'_>, plan: &Plan) -> Result<String, Error> {
    commit_request(
        ctx,
        Request {
            plan,
            image: &ImageChange::Keep,
        },
    )
    .await
}
async fn commit_request(ctx: Context<'_>, request: Request<'_>) -> Result<String, Error> {
    let plan = request.plan;
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
    if matches!(request.image, ImageChange::Upload(_)) {
        authorize_attachment(ctx, &target).await?;
    }
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
    execute_request(
        &ctx.data().store,
        &ctx.data().catalog,
        guild,
        bot,
        ctx.http(),
        request,
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
    execute_request(
        store,
        catalog,
        guild,
        bot,
        http,
        Request {
            plan,
            image: &ImageChange::Keep,
        },
    )
    .await
}
pub async fn execute_request(
    store: &super::storage::Store,
    catalog: &super::catalog::Catalog,
    guild: s::GuildId,
    bot: s::UserId,
    http: &s::Http,
    request: Request<'_>,
) -> Result<String, Error> {
    let plan = request.plan;
    let mut embeds = if plan.kind == "announcements" {
        vec![]
    } else {
        plan.document.embeds(plan.kind)?
    };
    let invite = if plan.kind == "links" {
        super::links::published_invite(&plan.document, plan.operation)?
    } else {
        None
    };
    let components = if plan.kind == "links" {
        super::links::invite_components(
            &plan.document,
            invite.as_ref().map_or("", |i| i.custom_id.as_str()),
        )?
    } else {
        vec![]
    };
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
    let mut announcement = if plan.kind == "announcements" {
        let doc = if plan.delete {
            super::announcements::without_legacy_identity(plan.document.clone())
        } else {
            super::announcements::build(http, guild, &plan.document).await?
        };
        Some(doc)
    } else {
        None
    };
    let mut missing = false;
    let mut original = None;
    if let Some(publication) = &existing {
        ensure!(
            publication.channel == plan.channel,
            "Canal configurado mudou; referências preservadas, reconcilie o registro antes de continuar"
        );
        match plan.channel.message(http, publication.message).await {
            Ok(message) => {
                ensure!(
                    message.author.id == bot,
                    "Mensagem registrada não pertence ao bot"
                );
                original = Some(message);
            }
            Err(s::Error::Http(s::HttpError::UnsuccessfulRequest(response)))
                if response.status_code.as_u16() == 404 =>
            {
                let recreate_links = plan.kind == "links"
                    && plan.action == "publish"
                    && !plan.delete
                    && response.error.code == 10008;
                if plan.kind == "links" && !plan.delete && !recreate_links {
                    return Err(s::Error::Http(s::HttpError::UnsuccessfulRequest(response)).into());
                }
                ensure!(
                    plan.delete || recreate_links,
                    "Mensagem original não encontrada (404). IDs preservados; use Remove para reconciliar antes de uma nova publicação"
                );
                missing = true;
                if recreate_links {
                    state
                        .archived_publications
                        .push(super::storage::ArchivedPublication {
                            key: plan.key.clone(),
                            publication: publication.clone(),
                            revision: state.revision,
                        });
                    tracing::info!(%guild, module = plan.kind, action = plan.action, stage = "recover_missing_publication", http_status = 404, discord_code = 10008, message = %publication.message, "Mensagem apagada; referência arquivada, criando substituta após confirmação de Publish");
                }
            }
            Err(error) => return Err(error.into()),
        }
    }
    let attachments = if let Some(doc) = &mut announcement {
        if plan.delete {
            doc.image.clear();
        }
        let attachments = if plan.delete {
            s::EditAttachments::new()
        } else if let Some(original) = &original {
            media::edit_attachments(
                doc,
                request.image,
                original,
                state.announcement_images.get(&plan.key),
            )?
        } else {
            request.image.apply(doc)?;
            s::EditAttachments::new()
        };
        embeds = doc.embeds(plan.kind)?;
        Some(attachments)
    } else {
        None
    };
    let document = announcement.as_ref().unwrap_or(&plan.document);
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
        if plan.kind == "links" {
            state.link_invites.remove(&plan.key);
        }
        if plan.kind == "announcements" {
            state.documents.remove(&plan.key);
            state.announcement_images.remove(&plan.key);
        }
    } else {
        let message = if let Some(publication) = existing.filter(|_| !missing) {
            let mut edit = s::EditMessage::new()
                .content(&document.text)
                .embeds(embeds)
                .allowed_mentions(s::CreateAllowedMentions::new());
            if plan.kind == "links" {
                edit = edit.components(components);
            }
            if let Some(attachments) = attachments {
                edit = edit.attachments(attachments);
            }
            plan.channel
                .edit_message(http, publication.message, edit)
                .await?
        } else {
            let mut create = s::CreateMessage::new()
                .content(&document.text)
                .embeds(embeds)
                .allowed_mentions(s::CreateAllowedMentions::new());
            if plan.kind == "links" {
                create = create.components(components);
            }
            if plan.kind == "announcements" {
                for file in request.image.files() {
                    create = create.add_file(file);
                }
            }
            plan.channel.send_message(http, create).await?
        };
        state.publications.insert(
            plan.key.clone(),
            Publication {
                channel: plan.channel,
                message: message.id,
            },
        );
        if plan.kind == "announcements" {
            if let Some(image) = media::published(document, &message)? {
                state.announcement_images.insert(plan.key.clone(), image);
            } else {
                state.announcement_images.remove(&plan.key);
            }
            state.documents.insert(plan.key.clone(), document.clone());
        }
        if plan.kind == "links" {
            if let Some(invite) = invite {
                state.link_invites.insert(plan.key.clone(), invite);
            } else {
                state.link_invites.remove(&plan.key);
            }
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
        let recovery = if missing {
            "A mensagem anterior foi apagada. Uma nova publicação foi criada e o ID anterior foi arquivado.\n"
        } else {
            ""
        };
        Ok(format!(
            "{recovery}Conteúdo publicado; estado salvo nesta guilda. Identificador: `{}`\nMessage ID: `{}`\n[Ver mensagem]({})",
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
