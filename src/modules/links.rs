use crate::{
    core::{Context, Error, permissions},
    services::{
        catalog::Board,
        components_v2,
        content::{Document, Entry},
        links as service,
        publishing::{self, Plan},
        ui,
    },
};
use anyhow::{Context as _, ensure};
use poise::serenity_prelude as s;
use std::time::Duration;

#[derive(Default, poise::Modal)]
#[name = "Adicionar link"]
pub struct LinkForm {
    #[name = "ID"]
    #[max_length = 64]
    pub id: String,
    #[name = "Título"]
    #[max_length = 200]
    pub title: String,
    #[name = "Descrição"]
    #[paragraph]
    #[max_length = 2000]
    pub description: Option<String>,
    #[name = "URL"]
    #[max_length = 2048]
    pub url: String,
    #[name = "Emoji"]
    #[max_length = 32]
    pub emoji: Option<String>,
}
impl LinkForm {
    pub fn entry(self) -> Result<Entry, Error> {
        let entry = Entry {
            id: self.id,
            title: self.title,
            description: self.description.unwrap_or_default(),
            url: self.url,
            emoji: self.emoji.unwrap_or_default(),
            ..Default::default()
        };
        for (field, value, max) in [
            ("Título", entry.title.as_str(), 200),
            ("Descrição", entry.description.as_str(), 2000),
            ("Emoji", entry.emoji.as_str(), 32),
        ] {
            ensure!(
                value.encode_utf16().count() <= max,
                "{field} excede o limite de {max} caracteres do formulário"
            );
        }
        entry.validate_link()?;
        Ok(entry)
    }
}
#[derive(poise::Modal)]
#[name = "Editar quadro de Links"]
struct BoardForm {
    #[name = "Título"]
    #[max_length = 256]
    title: String,
    #[name = "Introdução"]
    #[paragraph]
    #[max_length = 4000]
    description: Option<String>,
    #[name = "Cor hexadecimal"]
    color: String,
    #[name = "Imagem (URL)"]
    image: Option<String>,
    #[name = "Rodapé"]
    #[max_length = 2048]
    footer: Option<String>,
}
async fn authorize(ctx: Context<'_>, action: &str) -> Result<permissions::Authority, Error> {
    permissions::action(ctx, "links", action)?;
    permissions::administrative(ctx, s::Permissions::MANAGE_MESSAGES).await
}

pub async fn handle(ctx: Context<'_>, action: &str, id: Option<String>) -> Result<(), Error> {
    let auth = authorize(ctx, action).await?;
    if action == "update" || action == "reset" {
        return super::content_commands::collection(ctx, "links", action, None).await;
    }
    if action == "remove" {
        return remove(
            ctx,
            service::removal_id(action, id.as_deref()),
            auth.guild.icon_url(),
        )
        .await;
    }
    // ID is deliberately ignored for every other action, including Add.
    let Some(board) = super::content_commands::select_board(ctx, None).await? else {
        return Ok(());
    };
    let snapshot = ctx.data().catalog.snapshot(auth.guild.id).await?;
    let board = snapshot
        .links
        .boards
        .iter()
        .find(|b| b.id == board.id)
        .context("Quadro alterado; selecione novamente")?
        .clone();
    let icon = auth.guild.icon_url();
    match action {
        "add" => add(ctx, board, snapshot.revision, icon).await,
        "preview" => {
            show(
                ctx,
                service::with_guild_icon(board.document(), icon),
                None,
                false,
            )
            .await?;
            Ok(())
        }
        "publish" => publish(ctx, board, snapshot.revision, icon).await,
        _ => anyhow::bail!("Ação Links inválida"),
    }
}
async fn add(
    ctx: Context<'_>,
    board: Board,
    revision: u64,
    icon: Option<String>,
) -> Result<(), Error> {
    let Some(form) = ui::open_form(ctx, LinkForm::default()).await? else {
        return Ok(());
    };
    let entry = form.entry()?;
    ensure!(
        !board.links.iter().any(|e| e.id == entry.id),
        "Link ID já existe neste quadro"
    );
    let mut candidate = board.document();
    candidate.entries.push(entry.clone());
    components_v2::build(&candidate, "links-preview", vec![])?;
    let mut preview = board.document();
    preview.entries = vec![entry.clone()];
    if show(
        ctx,
        service::with_guild_icon(preview, icon),
        Some("Salvar"),
        false,
    )
    .await?
    .is_none()
    {
        return Ok(());
    }
    authorize(ctx, "add").await?;
    let cfg = permissions::guild_config(ctx)?;
    let _guard = ctx.data().store.gate.lock().await;
    ensure!(
        ctx.data().catalog.snapshot(cfg.guild_id).await?.revision == revision,
        "Catálogo alterado; abra o formulário novamente"
    );
    let (source, backup) = ctx
        .data()
        .catalog
        .add_link(&ctx.data().config.root, cfg, &board.id, entry)
        .await?;
    tracing::info!(guild = %cfg.guild_id, action = "add", stage = "toml_write", board = board.id, source, backup = ?backup, "Link salvo em TOML específico e memória");
    ctx.say("Link salvo no TOML desta guilda e catálogo recarregado. Nada publicado; use /links ação:publish.").await?;
    Ok(())
}
async fn remove(ctx: Context<'_>, id: Option<&str>, icon: Option<String>) -> Result<(), Error> {
    let cfg = permissions::guild_config(ctx)?;
    let snapshot = ctx.data().catalog.snapshot(cfg.guild_id).await?;
    let matches = service::matching_links(&snapshot.links, id);
    ensure!(!matches.is_empty(), "Link ID não encontrado nesta guilda");
    let selected = if id.is_some() && matches.len() == 1 {
        service::link_token(&matches[0].0.id, &matches[0].1.id)
    } else {
        let options = matches
            .iter()
            .map(|(b, e)| {
                (
                    service::link_token(&b.id, &e.id),
                    format!("{} — {} ({})", e.title, e.id, b.id),
                )
            })
            .collect::<Vec<_>>();
        let Some(token) = ui::select(ctx, "Selecione o link a remover do TOML", &options).await?
        else {
            return Ok(());
        };
        token
    };
    let (board, entry) = matches
        .into_iter()
        .find(|(b, e)| service::link_token(&b.id, &e.id) == selected)
        .context("Seleção de link inválida")?;
    let mut preview = board.document();
    preview.entries = vec![entry.clone()];
    if show(
        ctx,
        service::with_guild_icon(preview, icon),
        Some("Remover link"),
        false,
    )
    .await?
    .is_none()
    {
        return Ok(());
    }
    authorize(ctx, "remove").await?;
    let _guard = ctx.data().store.gate.lock().await;
    ensure!(
        ctx.data().catalog.snapshot(cfg.guild_id).await?.revision == snapshot.revision,
        "Catálogo alterado; selecione o link novamente"
    );
    ctx.data()
        .catalog
        .remove_link(&ctx.data().config.root, cfg, &board.id, entry.clone())
        .await?;
    tracing::info!(guild = %cfg.guild_id, action = "remove", stage = "toml_write", board = board.id, entry = entry.id, "Somente o link selecionado foi removido; publicação preservada");
    ctx.say("Link removido somente do TOML específico e do catálogo em memória. Quadro e publicação preservados; use Publish para atualizar o Discord.").await?;
    Ok(())
}
async fn publish(
    ctx: Context<'_>,
    board: Board,
    revision: u64,
    icon: Option<String>,
) -> Result<(), Error> {
    let cfg = permissions::guild_config(ctx)?;
    let bot = ctx.serenity_context().cache.current_user().id;
    crate::services::publication_state::ensure_ready(
        &ctx.data().store,
        cfg.guild_id,
        bot,
        ctx.http(),
    )
    .await?;
    let state_revision = {
        let _guard = ctx.data().store.gate.lock().await;
        let state = ctx.data().store.read(cfg.guild_id)?;
        ensure!(
            state.pending.is_none(),
            "{}",
            crate::services::publication_state::describe(&state)
        );
        state.revision
    };
    let publication = {
        let _guard = ctx.data().store.gate.lock().await;
        ctx.data()
            .store
            .read(cfg.guild_id)?
            .publications
            .get(&board.publication_key())
            .cloned()
    };
    let replacement = if let Some(publication) = publication {
        match components_v2::get(ctx.http(), publication.channel, publication.message).await {
            Ok(message) => {
                ensure!(
                    message.author.id == ctx.serenity_context().cache.current_user().id
                        && message.guild_id.is_none_or(|g| g == cfg.guild_id),
                    "Mensagem original de outra autoria/guilda"
                );
                message.needs_replacement()
            }
            Err(error)
                if matches!(
                    crate::services::publication_state::rejection(&error),
                    Some(crate::services::publication_state::Rejection {
                        http_status: 404,
                        discord_code: 10008
                    })
                ) =>
            {
                false
            }
            Err(error) => return Err(error),
        }
    } else {
        false
    };
    let original = board.document();
    let Some(displayed) = show(
        ctx,
        service::with_guild_icon(original.clone(), icon),
        Some(if replacement {
            "Substituir publicação"
        } else {
            "Publicar"
        }),
        true,
    )
    .await?
    else {
        return Ok(());
    };
    authorize(ctx, "publish").await?;
    // The automatically selected guild thumbnail is presentation, not a TOML customization.
    let mut edited = displayed.clone();
    edited.thumbnail = original.thumbnail.clone();
    let (memory_revision, json_revision) = {
        let _guard = ctx.data().store.gate.lock().await;
        let state = ctx.data().store.read(cfg.guild_id)?;
        ensure!(
            state.revision == state_revision && state.pending.is_none(),
            "Estado alterado; abra uma nova prévia"
        );
        ensure!(
            ctx.data().catalog.snapshot(cfg.guild_id).await?.revision == revision,
            "Catálogo alterado; abra uma nova prévia"
        );
        if edited != original {
            ctx.data()
                .catalog
                .edit_board(&ctx.data().config.root, cfg, &board.id, original, edited)
                .await?;
        }
        (
            ctx.data().catalog.snapshot(cfg.guild_id).await?.revision,
            state.revision,
        )
    };
    let plan = Plan {
        operation: ctx.id(),
        kind: "links",
        action: "publish".into(),
        key: board.publication_key(),
        document: displayed,
        channel: cfg.channel(&board.channel)?,
        revision: json_revision,
        content_revision: Some(memory_revision),
        delete: false,
    };
    let result = if replacement {
        publishing::commit_links_replacement(ctx, &plan).await?
    } else {
        publishing::commit(ctx, &plan).await?
    };
    ctx.say(result).await?;
    Ok(())
}

/// Shared private renderer for Preview, Add/Remove confirmation and Publish with editing.
async fn show(
    ctx: Context<'_>,
    mut doc: Document,
    accept: Option<&str>,
    editable: bool,
) -> Result<Option<Document>, Error> {
    let copy = format!("links-preview-copy:{}", ctx.id());
    let confirm = format!("links-accept:{}", ctx.id());
    let cancel = format!("links-cancel:{}", ctx.id());
    let edit = format!("links-edit:{}", ctx.id());
    let deadline = tokio::time::Instant::now() + Duration::from_secs(120);
    loop {
        let mut buttons = vec![];
        if let Some(label) = accept {
            if editable {
                buttons.push(s::CreateButton::new(&edit).label("Editar"));
            }
            buttons.push(
                s::CreateButton::new(&confirm)
                    .label(label)
                    .style(s::ButtonStyle::Primary),
            );
            buttons.push(s::CreateButton::new(&cancel).label("Cancelar"));
        }
        let interactive = accept.is_some() || doc.discord_invite()?.is_some();
        let mut payload = components_v2::build(&doc, &copy, buttons)?;
        if accept == Some("Publicar") {
            payload = payload.notice("Prévia privada. Publicar cria ou atualiza a mensagem original. Mensagens V1 serão convertidas para Components V2; essa conversão é irreversível nessa mensagem. Cancelar não altera a publicação.")?;
        }
        if accept == Some("Substituir publicação") {
            payload = payload.notice("A mensagem original contém stickers não editáveis. Confirmar cria uma nova publicação V2, registra seu novo ID somente após sucesso e preserva a mensagem anterior e sua referência arquivada. Cancelar não altera nenhuma publicação.")?;
        }
        let message = components_v2::preview(ctx, &payload).await?;
        if !interactive {
            return Ok(None);
        }
        loop {
            let interaction =
                s::ComponentInteractionCollector::new(ctx.serenity_context().shard.clone())
                    .message_id(message.id)
                    .author_id(ctx.author().id)
                    // The persistent Gateway handler responds to this button, including in previews.
                    .filter(|interaction| interaction.data.custom_id != service::INVITE_FRIENDS_ID)
                    .timeout(deadline.saturating_duration_since(tokio::time::Instant::now()))
                    .await;
            let Some(interaction) = interaction else {
                components_v2::close_preview(ctx, &message, &doc).await?;
                if accept.is_some() {
                    ctx.say("Sessão expirada; nada salvo ou publicado.").await?;
                }
                return Ok(None);
            };
            if interaction.data.custom_id == copy {
                let invite = doc.discord_invite()?.context("Convite ausente")?;
                components_v2::private_http(interaction.create_response(
                    ctx.http(),
                    s::CreateInteractionResponse::Message(service::copy_response(&invite.url)?),
                ))
                .await?;
                continue;
            }
            if interaction.data.custom_id == edit && editable {
                let form = BoardForm {
                    title: doc.title.clone(),
                    description: Some(doc.description.clone()),
                    color: format!("{:06X}", doc.color),
                    image: Some(doc.image.clone()),
                    footer: Some(doc.footer.clone()),
                };
                let updated = poise::execute_modal_on_component_interaction(
                    ctx,
                    interaction,
                    Some(form),
                    Some(deadline.saturating_duration_since(tokio::time::Instant::now())),
                )
                .await?;
                components_v2::close_preview(ctx, &message, &doc).await?;
                let Some(form) = updated else {
                    return Ok(None);
                };
                ensure!(
                    tokio::time::Instant::now() < deadline,
                    "Sessão expirada; nada salvo"
                );
                doc.title = form.title;
                doc.description = form.description.unwrap_or_default();
                doc.color = u32::from_str_radix(form.color.trim().trim_start_matches('#'), 16)
                    .context("Cor hexadecimal inválida")?;
                doc.image = form.image.unwrap_or_default();
                doc.footer = form.footer.unwrap_or_default();
                components_v2::build(&doc, &copy, vec![])?;
                break;
            }
            interaction
                .create_response(ctx.http(), s::CreateInteractionResponse::Acknowledge)
                .await?;
            components_v2::close_preview(ctx, &message, &doc).await?;
            if interaction.data.custom_id == confirm && accept.is_some() {
                return Ok(Some(doc));
            }
            ctx.say("Operação cancelada; nada salvo ou publicado.")
                .await?;
            return Ok(None);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use poise::Modal;

    #[test]
    fn add_modal_has_exactly_five_native_text_fields_and_required_flags() {
        let modal = serde_json::to_value(LinkForm::create(None, "test".into())).unwrap();
        assert_eq!(modal["type"], 9);
        let fields = modal["data"]["components"].as_array().unwrap();
        assert_eq!(fields.len(), 5);
        for (row, (name, required)) in fields.iter().zip([
            ("ID", true),
            ("Título", true),
            ("Descrição", false),
            ("URL", true),
            ("Emoji", false),
        ]) {
            assert_eq!(row["components"][0]["label"], name);
            assert_eq!(row["components"][0]["required"], required);
            assert_eq!(row["components"][0]["type"], 4);
        }
    }

    fn form() -> LinkForm {
        LinkForm {
            id: "whatsapp".into(),
            title: "Grupo".into(),
            url: "https://chat.whatsapp.com/EXEMPLO".into(),
            ..Default::default()
        }
    }
    #[test]
    fn add_validates_five_fields_without_requiring_optional_description_or_emoji() {
        assert!(form().entry().is_ok());
        let mut f = form();
        f.id = "../unsafe".into();
        assert!(f.entry().is_err());
        let mut f = form();
        f.title = " ".into();
        assert!(f.entry().is_err());
        let mut f = form();
        f.url = "javascript:alert(1)".into();
        assert!(f.entry().is_err());
        let mut f = form();
        f.description = Some("x".repeat(2001));
        assert!(f.entry().is_err());
        let mut f = form();
        f.emoji = Some("x".repeat(33));
        assert!(f.entry().is_err());
        let mut f = form();
        f.url = "https://discord.gg/EXEMPLO".into();
        assert!(f.entry().unwrap().is_discord_invite().unwrap());
    }
}
