use crate::{
    core::{Context, Error, permissions},
    modules::actions::{AnnouncementAction, LinksAction, RulesAction},
    services::{
        content::Document,
        publishing::{self as p, Plan},
        ui,
    },
};
use anyhow::{Context as _, ensure};
use poise::serenity_prelude as s;
use std::time::Duration;

#[derive(poise::Modal)]
#[name = "Anúncio"]
pub struct AnnouncementForm {
    #[name = "Título"]
    #[max_length = 256]
    title: String,
    #[name = "Conteúdo"]
    #[paragraph]
    #[max_length = 4000]
    description: String,
    #[name = "Cor hexadecimal (#5865F2)"]
    color: String,
    #[name = "URL da imagem"]
    image: Option<String>,
    #[name = "Rodapé"]
    #[max_length = 2048]
    footer: Option<String>,
}
pub async fn announcement_modal(
    ctx: Context<'_>,
    interaction: Option<&s::ComponentInteraction>,
    doc: &Document,
) -> Result<Option<Document>, Error> {
    let form = AnnouncementForm {
        title: doc.title.clone(),
        description: doc.description.clone(),
        color: format!("{:06X}", doc.color),
        image: Some(doc.image.clone()),
        footer: Some(doc.footer.clone()),
    };
    let value = if let Some(interaction) = interaction {
        poise::execute_modal_on_component_interaction(
            ctx,
            interaction.clone(),
            Some(form),
            Some(Duration::from_secs(180)),
        )
        .await?
    } else {
        ui::open_form(ctx, form).await?
    };
    value
        .map(|v| -> Result<Document, Error> {
            Ok(Document {
                title: v.title,
                description: v.description,
                color: u32::from_str_radix(v.color.trim().trim_start_matches('#'), 16)
                    .context("Cor hexadecimal inválida")?,
                image: v.image.unwrap_or_default(),
                footer: v.footer.unwrap_or_default(),
                ..doc.clone()
            })
        })
        .transpose()
}

/// Regras: prévia por padrão; Update relê TOML, Publish modifica o Discord.
#[poise::command(
    slash_command,
    guild_only,
    ephemeral,
    default_member_permissions = "MANAGE_MESSAGES"
)]
pub async fn rules(
    ctx: Context<'_>,
    #[description = "Ação (padrão: preview)"] ação: Option<RulesAction>,
) -> Result<(), Error> {
    ctx.defer_ephemeral().await?;
    let cfg = permissions::guild_config(ctx)?;
    let action = cfg.select_action(
        "rules",
        ação.map(|v| v.as_str()),
        ctx.data().config.global.modules,
    )?;
    collection(ctx, "rules", &action, None).await
}

/// Catálogo de Links: escolha explicitamente a ação; ID é usado somente em Remove.
#[poise::command(
    slash_command,
    guild_only,
    ephemeral,
    rename = "links",
    default_member_permissions = "MANAGE_MESSAGES"
)]
async fn links_command(
    ctx: Context<'_>,
    #[description = "Operação obrigatória do catálogo de Links"] ação: String,
    #[description = "ID do link, somente para Remove"]
    #[autocomplete = "link_autocomplete"]
    id: Option<String>,
) -> Result<(), Error> {
    ctx.defer_ephemeral().await?;
    let cfg = permissions::guild_config(ctx)?;
    let action = cfg.select_action("links", Some(&ação), ctx.data().config.global.modules)?;
    crate::modules::links::handle(ctx, &action, id).await
}
// Poise's derived choices use INTEGER indices. Links intentionally registers STRING values.
pub fn links() -> poise::Command<crate::core::Data, Error> {
    let mut command = links_command();
    let action = command
        .parameters
        .iter_mut()
        .find(|p| p.name == "ação")
        .unwrap();
    action.choices = <LinksAction as poise::ChoiceParameter>::list();
    action.type_setter = Some(crate::services::links::action_option);
    command
}
async fn link_autocomplete(ctx: Context<'_>, partial: &str) -> Vec<s::AutocompleteChoice> {
    let Ok(cfg) = permissions::guild_config(ctx) else {
        return vec![];
    };
    if !cfg.command_enabled("links.remove", ctx.data().config.global.modules)
        || permissions::administrative(ctx, s::Permissions::MANAGE_MESSAGES)
            .await
            .is_err()
    {
        return vec![];
    }
    let Ok(snapshot) = ctx.data().catalog.snapshot(cfg.guild_id).await else {
        return vec![];
    };
    crate::services::links::autocomplete(&snapshot.links, partial)
}

pub(crate) async fn select_board(
    ctx: Context<'_>,
    requested: Option<String>,
) -> Result<Option<crate::services::catalog::Board>, Error> {
    let cfg = permissions::guild_config(ctx)?;
    let snapshot = ctx.data().catalog.snapshot(cfg.guild_id).await?;
    let id = if let Some(id) = requested {
        id
    } else if let Some(board) = crate::services::links::preferred_board(&snapshot.links.boards) {
        board.id.clone()
    } else {
        let options = snapshot
            .links
            .boards
            .iter()
            .map(|b| (b.id.clone(), format!("{} — {}", b.title, b.id)))
            .collect::<Vec<_>>();
        let Some(id) = ui::select(ctx, "Selecione o quadro de links", &options).await? else {
            return Ok(None);
        };
        id
    };
    Ok(Some(
        snapshot
            .links
            .boards
            .into_iter()
            .find(|b| b.id == id)
            .context("Board ID não encontrado nesta guilda; selecione um quadro disponível")?,
    ))
}
pub(crate) async fn collection(
    ctx: Context<'_>,
    kind: &'static str,
    action: &str,
    board: Option<String>,
) -> Result<(), Error> {
    let cfg = permissions::guild_config(ctx)?;
    permissions::action(ctx, kind, action)?;
    permissions::administrative(ctx, s::Permissions::MANAGE_MESSAGES).await?;
    if action == "update" {
        let _guard = ctx.data().store.gate.lock().await;
        let source = ctx
            .data()
            .catalog
            .reload(&ctx.data().config.root, cfg, kind)
            .await?;
        tracing::info!(guild = %cfg.guild_id, module = kind, action, stage = "reload", source, "TOML validado e memória substituída");
        ctx.say(format!(
            "{kind}: memória recarregada de `{source}`. Nada publicado."
        ))
        .await?;
        return Ok(());
    }
    if action == "reset" {
        return reset(ctx, kind).await;
    }
    let selected = if kind == "links" {
        let Some(board) = select_board(ctx, board).await? else {
            return Ok(());
        };
        Some(board)
    } else {
        None
    };
    let _guard = ctx.data().store.gate.lock().await;
    let snapshot = ctx.data().catalog.snapshot(cfg.guild_id).await?;
    let state = ctx.data().store.read(cfg.guild_id)?;
    let (doc, key, channel) = if let Some(board) = selected {
        // Resolve again under the write gate, after any menu interaction.
        let board = snapshot
            .links
            .boards
            .iter()
            .find(|b| b.id == board.id)
            .context("Quadro removido da configuração; selecione novamente")?;
        (
            board.document(),
            board.publication_key(),
            cfg.channel(&board.channel)?,
        )
    } else {
        (
            snapshot.rules.clone(),
            "rules".into(),
            cfg.channel("rules")?,
        )
    };
    drop(_guard);
    let source = snapshot
        .sources
        .get(kind)
        .map(String::as_str)
        .unwrap_or("cache legado");
    ctx.say(format!(
        "Origem: `{source}` • revisão em memória {} • TOML tem prioridade sobre JSON legado.",
        snapshot.revision
    ))
    .await?;
    if action == "preview" {
        ctx.send(
            poise::CreateReply {
                embeds: doc.embeds(kind)?,
                ..Default::default()
            }
            .content(if doc.text.is_empty() {
                "Prévia privada; nada publicado."
            } else {
                &doc.text
            })
            .ephemeral(true)
            .allowed_mentions(s::CreateAllowedMentions::new()),
        )
        .await?;
        return Ok(());
    }
    ensure!(
        action != "remove" || state.publications.contains_key(&key),
        "Este recurso não possui publicação nesta guilda"
    );
    p::preview(
        ctx,
        Plan {
            operation: ctx.id(),
            kind,
            action: action.into(),
            key,
            document: doc,
            channel,
            revision: state.revision,
            content_revision: Some(snapshot.revision),
            delete: action == "remove",
        },
    )
    .await
}
async fn reset(ctx: Context<'_>, kind: &'static str) -> Result<(), Error> {
    let cfg = permissions::guild_config(ctx)?;
    let revision = ctx.data().catalog.snapshot(cfg.guild_id).await?.revision;
    if !ui::confirm(ctx, &format!("Restaurar {kind}.toml compartilhado para esta guilda? O arquivo específico será preservado em backup. A memória será recarregada; as publicações e seus IDs serão mantidos."), "Restaurar TOML", None).await? { return Ok(()); }
    permissions::action(ctx, kind, "reset")?;
    permissions::administrative(ctx, s::Permissions::MANAGE_MESSAGES).await?;
    let _guard = ctx.data().store.gate.lock().await;
    ensure!(
        ctx.data().catalog.snapshot(cfg.guild_id).await?.revision == revision,
        "Conteúdo alterado durante a confirmação; execute Reset novamente"
    );
    ensure!(
        ctx.data().store.read(cfg.guild_id)?.pending.is_none(),
        "Existe operação pendente; reconcilie antes do Reset"
    );
    let (source, backup) = ctx
        .data()
        .catalog
        .reset(&ctx.data().config.root, cfg, kind)
        .await?;
    tracing::info!(guild = %cfg.guild_id, module = kind, action = "reset", stage = "toml_write", source, backup = ?backup, "Defaults restaurados e memória atualizada");
    ctx.say(format!("Restaurado `{source}` e recarregado em memória. {} Nada publicado; use ação:publish para atualizar a mensagem.", if backup.is_some() { "Arquivo anterior preservado em backup." } else { "Arquivo específico criado." })).await?;
    Ok(())
}

/// Anúncio gerenciado: Create por padrão, Edit e Remove preservam identificadores.
#[poise::command(
    slash_command,
    guild_only,
    ephemeral,
    default_member_permissions = "MANAGE_MESSAGES"
)]
pub async fn announcement(
    ctx: Context<'_>,
    #[description = "Ação (padrão: create)"] ação: Option<AnnouncementAction>,
    #[description = "Mensagem; quando ausente abre formulário"] mensagem: Option<String>,
    #[description = "Anúncio gerenciado"]
    #[autocomplete = "announcement_autocomplete"]
    identificador: Option<String>,
) -> Result<(), Error> {
    ctx.defer_ephemeral().await?;
    let cfg = permissions::guild_config(ctx)?;
    let action = cfg.select_action(
        "announcement",
        ação.map(|v| v.as_str()),
        ctx.data().config.global.modules,
    )?;
    let supplied: Vec<_> = [
        ("mensagem", mensagem.is_some()),
        ("identificador", identificador.is_some()),
    ]
    .into_iter()
    .filter_map(|(k, v)| v.then_some(k))
    .collect();
    permissions::conditional_parameters("announcement", &action, &supplied)?;
    permissions::action(ctx, "announcement", &action)?;
    permissions::administrative(ctx, s::Permissions::MANAGE_MESSAGES).await?;
    let state = {
        let _guard = ctx.data().store.gate.lock().await;
        ctx.data().store.read(cfg.guild_id)?
    };
    let (key, base) = if action == "create" {
        (
            crate::services::announcements::new_id(ctx.id()),
            crate::services::storage::template(
                &ctx.data().templates,
                cfg,
                "announcements",
                &state,
            )?,
        )
    } else {
        let id = if let Some(id) = identificador {
            crate::services::announcements::resolve_in_guild(&state, cfg.guild_id, &id)?
        } else {
            let mut options: Vec<_> = state
                .publications
                .keys()
                .filter(|k| k.starts_with("announcement-"))
                .map(|k| {
                    (
                        k.clone(),
                        format!(
                            "{} — {k}",
                            crate::services::announcements::summary(&state, k)
                        ),
                    )
                })
                .collect();
            options.sort();
            let Some(id) = ui::select(ctx, "Selecione o anúncio gerenciado", &options).await?
            else {
                return Ok(());
            };
            id
        };
        let doc = state
            .documents
            .get(&id)
            .context("Registro de anúncio incompleto")?
            .clone();
        (id, doc)
    };
    let doc = if action == "remove" {
        base
    } else if let Some(message) = mensagem {
        crate::services::announcements::with_message(&base, &message)?
    } else {
        let Some(doc) = announcement_modal(ctx, None, &base).await? else {
            return Ok(());
        };
        doc
    };
    p::preview(
        ctx,
        Plan {
            operation: ctx.id(),
            kind: "announcements",
            action: action.clone(),
            key,
            document: doc,
            channel: cfg.channel("announcements")?,
            revision: state.revision,
            content_revision: None,
            delete: action == "remove",
        },
    )
    .await
}
async fn announcement_autocomplete(ctx: Context<'_>, partial: &str) -> Vec<s::AutocompleteChoice> {
    let Ok(cfg) = permissions::guild_config(ctx) else {
        return vec![];
    };
    if !cfg.command_enabled("announcement", ctx.data().config.global.modules)
        || permissions::administrative(ctx, s::Permissions::MANAGE_MESSAGES)
            .await
            .is_err()
    {
        return vec![];
    }
    let _guard = ctx.data().store.gate.lock().await;
    let Ok(state) = ctx.data().store.read(cfg.guild_id) else {
        return vec![];
    };
    crate::services::announcements::choices(&state, partial)
}
