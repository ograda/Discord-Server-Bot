use crate::{
    core::{Context, Error, permissions},
    services::{
        content::{Document, Entry},
        publishing as p,
    },
};
use anyhow::{Context as _, ensure};
use poise::serenity_prelude as s;
use std::time::Duration;

// Confirma o slash antes das consultas HTTP e abre o modal em interação nova.
async fn open_form<M: poise::Modal + Send + Sync>(
    ctx: Context<'_>,
    form: M,
) -> Result<Option<M>, Error> {
    let custom_id = format!("form:{}", ctx.id());
    let reply = ctx
        .send(
            poise::CreateReply::default()
                .content("Abra o formulário para preparar o conteúdo (90 segundos).")
                .ephemeral(true)
                .components(vec![s::CreateActionRow::Buttons(vec![
                    s::CreateButton::new(&custom_id)
                        .label("Preencher formulário")
                        .style(s::ButtonStyle::Primary),
                ])]),
        )
        .await?;
    let message = reply.message().await?;
    let interaction = message
        .await_component_interaction(ctx.serenity_context())
        .author_id(ctx.author().id)
        .timeout(Duration::from_secs(90))
        .await;
    let result = if let Some(interaction) = interaction {
        poise::execute_modal_on_component_interaction(
            ctx,
            interaction,
            Some(form),
            Some(Duration::from_secs(180)),
        )
        .await?
    } else {
        None
    };
    reply
        .edit(
            ctx,
            poise::CreateReply::default()
                .content(if result.is_some() {
                    "Formulário recebido."
                } else {
                    "Formulário cancelado ou expirado. Execute novamente."
                })
                .components(vec![]),
        )
        .await?;
    Ok(result)
}

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
        open_form(ctx, form).await?
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
#[derive(poise::Modal)]
#[name = "Regra ou link"]
struct EntryForm {
    #[name = "Título"]
    #[max_length = 200]
    title: String,
    #[name = "Descrição"]
    #[paragraph]
    #[max_length = 900]
    description: String,
    #[name = "Emoji"]
    #[max_length = 32]
    emoji: Option<String>,
    #[name = "URL (somente links)"]
    url: Option<String>,
}

#[derive(poise::Modal)]
#[name = "Editar item da prévia"]
struct CollectionForm {
    #[name = "ID do item (consulte list)"]
    id: String,
    #[name = "Título"]
    #[max_length = 200]
    title: String,
    #[name = "Descrição"]
    #[paragraph]
    #[max_length = 900]
    description: String,
    #[name = "Emoji"]
    #[max_length = 32]
    emoji: Option<String>,
    #[name = "URL (somente links)"]
    url: Option<String>,
}
pub async fn collection_modal(
    ctx: Context<'_>,
    interaction: &s::ComponentInteraction,
    doc: &Document,
) -> Result<Option<Document>, Error> {
    let entry = doc.entries.first().context("Nenhum item")?;
    let defaults = CollectionForm {
        id: entry.id.clone(),
        title: entry.title.clone(),
        description: entry.description.clone(),
        emoji: Some(entry.emoji.clone()),
        url: Some(entry.url.clone()),
    };
    let Some(form) = poise::execute_modal_on_component_interaction(
        ctx,
        interaction.clone(),
        Some(defaults),
        Some(Duration::from_secs(180)),
    )
    .await?
    else {
        return Ok(None);
    };
    let mut doc = doc.clone();
    let entry = doc
        .entries
        .iter_mut()
        .find(|e| e.id == form.id)
        .context("Item não encontrado")?;
    entry.title = form.title;
    entry.description = form.description;
    entry.emoji = form.emoji.unwrap_or_default();
    entry.url = form.url.unwrap_or_default();
    Ok(Some(doc))
}
async fn entry_modal(ctx: Context<'_>, entry: &Entry) -> Result<Option<Entry>, Error> {
    let form = EntryForm {
        title: entry.title.clone(),
        description: entry.description.clone(),
        emoji: Some(entry.emoji.clone()),
        url: Some(entry.url.clone()),
    };
    Ok(open_form(ctx, form).await?.map(|v| Entry {
        title: v.title,
        description: v.description,
        emoji: v.emoji.unwrap_or_default(),
        url: v.url.unwrap_or_default(),
        ..entry.clone()
    }))
}

/// Anúncios gerenciados com prévia privada.
#[poise::command(
    slash_command,
    guild_only,
    ephemeral,
    subcommands(
        "announcement_create",
        "announcement_edit",
        "announcement_list",
        "announcement_delete"
    ),
    default_member_permissions = "MANAGE_MESSAGES"
)]
pub async fn announcements(_: Context<'_>) -> Result<(), Error> {
    Ok(())
}
/// Cria um anúncio por formulário.
#[poise::command(slash_command, guild_only, ephemeral, rename = "create")]
async fn announcement_create(ctx: Context<'_>) -> Result<(), Error> {
    ctx.defer_ephemeral().await?;
    let (doc, revision) = p::load(ctx, "announcements").await?;
    if let Some(doc) = announcement_modal(ctx, None, &doc).await? {
        p::preview(
            ctx,
            "announcements",
            crate::services::announcements::new_id(ctx.id()),
            doc,
            revision,
            false,
        )
        .await?;
    }
    Ok(())
}
async fn announcement(ctx: Context<'_>, id: String, delete: bool) -> Result<(), Error> {
    let channel = p::authorize(ctx, "announcements", true).await?;
    let guild = permissions::guild_config(ctx)?.guild_id;
    let state = {
        let _guard = ctx.data().store.gate.lock().await;
        ctx.data().store.read(guild)?
    };
    let id = crate::services::announcements::resolve(&state, &id)?;
    ensure!(
        state.pending.is_none(),
        "Há uma operação pendente de reconciliação nesta guilda; confira o estado antes de gerenciar anúncios."
    );
    let publication = &state.publications[&id];
    ensure!(
        publication.channel == channel,
        "O canal configurado mudou; reconcilie o registro do anúncio antes de continuar."
    );
    let auth = permissions::administrative(ctx, s::Permissions::MANAGE_MESSAGES).await?;
    let target = channel
        .to_channel(ctx.http())
        .await?
        .guild()
        .context("Canal indisponível")?;
    permissions::ensure_bot_channel_permissions(
        &auth,
        &target,
        s::Permissions::READ_MESSAGE_HISTORY,
    )?;
    let missing = match channel.message(ctx.http(), publication.message).await {
        Ok(message) => {
            ensure!(
                message.author.id == auth.bot.user.id,
                "Mensagem registrada não pertence ao bot; operação bloqueada"
            );
            false
        }
        Err(s::Error::Http(s::HttpError::UnsuccessfulRequest(response)))
            if response.status_code.as_u16() == 404 =>
        {
            true
        }
        Err(s::Error::Http(s::HttpError::UnsuccessfulRequest(response)))
            if response.status_code.as_u16() == 403 =>
        {
            anyhow::bail!("O bot perdeu acesso ao canal ou ao histórico da mensagem.")
        }
        Err(error) => return Err(error.into()),
    };
    if missing {
        ctx.say("A mensagem original foi excluída. Confirme a exclusão abaixo para reconciliar somente o registro local.").await?;
    }
    let delete = delete || missing;
    let doc = state
        .documents
        .get(&id)
        .context("Anúncio não encontrado; use list")?;
    let doc = if delete {
        Some(doc.clone())
    } else {
        announcement_modal(ctx, None, doc).await?
    };
    if let Some(doc) = doc {
        p::preview(ctx, "announcements", id, doc, state.revision, delete).await?;
    }
    Ok(())
}
/// Edita anúncio existente preservando o ID da mensagem.
#[poise::command(slash_command, guild_only, ephemeral, rename = "edit")]
async fn announcement_edit(
    ctx: Context<'_>,
    #[description = "Chave interna ou Message ID do anúncio"]
    #[autocomplete = "announcement_autocomplete"]
    id: String,
) -> Result<(), Error> {
    ctx.defer_ephemeral().await?;
    announcement(ctx, id, false).await
}
/// Exclui anúncio após confirmação.
#[poise::command(slash_command, guild_only, ephemeral, rename = "delete")]
async fn announcement_delete(
    ctx: Context<'_>,
    #[description = "Chave interna ou Message ID do anúncio"]
    #[autocomplete = "announcement_autocomplete"]
    id: String,
) -> Result<(), Error> {
    ctx.defer_ephemeral().await?;
    announcement(ctx, id, true).await
}
/// Lista identificadores dos anúncios gerenciados.
#[poise::command(slash_command, guild_only, ephemeral, rename = "list")]
async fn announcement_list(ctx: Context<'_>) -> Result<(), Error> {
    ctx.defer_ephemeral().await?;
    p::authorize(ctx, "announcements", false).await?;
    let guild = permissions::guild_config(ctx)?.guild_id;
    let state = {
        let _guard = ctx.data().store.gate.lock().await;
        ctx.data().store.read(guild)?
    };
    let mut rows: Vec<_> = state
        .publications
        .iter()
        .filter(|(key, _)| key.starts_with("announcement-"))
        .map(|(key, value)| {
            format!(
                "**{}**\nChave: `{key}`\nMessage ID: `{}` • criado <t:{}:f>\n[Mensagem]({}) • {}",
                crate::services::announcements::summary(&state, key),
                value.message,
                value.message.created_at().unix_timestamp(),
                crate::services::announcements::message_link(guild, value),
                if state.pending.is_some() {
                    "reconciliação pendente"
                } else {
                    "registrado (existência conferida ao gerenciar)"
                }
            )
        })
        .collect();
    rows.sort();
    if rows.is_empty() {
        ctx.say("Nenhum anúncio gerenciado nesta guilda.").await?;
    }
    for row in rows {
        ctx.say(row).await?;
    }
    Ok(())
}

async fn announcement_autocomplete(ctx: Context<'_>, partial: &str) -> Vec<s::AutocompleteChoice> {
    let Ok(cfg) = permissions::guild_config(ctx) else {
        return vec![];
    };
    if permissions::administrative(ctx, s::Permissions::MANAGE_MESSAGES)
        .await
        .is_err()
    {
        return vec![];
    }
    let _guard = ctx.data().store.gate.lock().await;
    let Ok(state) = ctx.data().store.read(cfg.guild_id) else {
        return vec![];
    };
    let mut keys: Vec<_> = state
        .publications
        .keys()
        .filter(|key| key.starts_with("announcement-"))
        .collect();
    keys.sort();
    keys.into_iter()
        .filter(|key| {
            key.contains(partial)
                || state.publications[*key]
                    .message
                    .to_string()
                    .contains(partial)
                || crate::services::announcements::summary(&state, key)
                    .to_lowercase()
                    .contains(&partial.to_lowercase())
        })
        .take(25)
        .map(|key| {
            s::AutocompleteChoice::new(
                format!(
                    "{} — {key}",
                    crate::services::announcements::summary(&state, key)
                )
                .chars()
                .take(100)
                .collect::<String>(),
                key.clone(),
            )
        })
        .collect()
}
async fn list_rows(ctx: Context<'_>, rows: Vec<String>) -> Result<(), Error> {
    if rows.is_empty() {
        ctx.say("Nenhum item configurado.").await?;
    }
    for chunk in rows.chunks(5) {
        ctx.say(chunk.join("\n")).await?;
    }
    Ok(())
}
async fn collection(ctx: Context<'_>, kind: &str, action: &str) -> Result<(), Error> {
    ctx.defer_ephemeral().await?;
    let (doc, revision) = p::load(ctx, kind).await?;
    if action == "list" {
        return list_rows(
            ctx,
            doc.entries
                .iter()
                .enumerate()
                .map(|(n, e)| format!("{} • {} • {}", n + 1, e.id, e.title))
                .collect(),
        )
        .await;
    }
    if action == "preview" {
        ctx.send(
            poise::CreateReply {
                embeds: doc.embeds(kind)?,
                ..Default::default()
            }
            .content("Prévia • nada publicado")
            .ephemeral(true),
        )
        .await?;
        return Ok(());
    }
    let guild = permissions::guild_config(ctx)?.guild_id;
    let exists = {
        let _guard = ctx.data().store.gate.lock().await;
        ctx.data()
            .store
            .read(guild)?
            .publications
            .contains_key(kind)
    };
    if action == "update" {
        ensure!(exists, "Não há publicação. Use publish.");
    }
    if exists && action == "publish" {
        ctx.say("Já existe publicação. A confirmação atualizará a mesma mensagem.")
            .await?;
    }
    p::preview(ctx, kind, kind.into(), doc, revision, false).await
}
async fn change_entry(ctx: Context<'_>, kind: &str, action: &str, id: String) -> Result<(), Error> {
    ctx.defer_ephemeral().await?;
    let (mut doc, revision) = p::load(ctx, kind).await?;
    let index = doc.entries.iter().position(|e| e.id == id).or_else(|| {
        if kind == "rules" {
            id.parse::<usize>()
                .ok()
                .and_then(|n| n.checked_sub(1))
                .filter(|n| *n < doc.entries.len())
        } else {
            None
        }
    });
    if action == "add" {
        ensure!(
            crate::config::registry::valid_alias(&id) && !doc.entries.iter().any(|e| e.id == id),
            "ID inválido ou duplicado"
        );
        let entry = Entry {
            id,
            order: doc
                .entries
                .iter()
                .map(|e| e.order)
                .max()
                .unwrap_or(0)
                .checked_add(1)
                .context("Limite de ordem atingido")?,
            ..Default::default()
        };
        let Some(entry) = entry_modal(ctx, &entry).await? else {
            return Ok(());
        };
        doc.entries.push(entry);
    } else {
        let index = index.context("Item não encontrado")?;
        if action == "remove" {
            doc.entries.remove(index);
        } else {
            let Some(entry) = entry_modal(ctx, &doc.entries[index]).await? else {
                return Ok(());
            };
            doc.entries[index] = entry;
        }
    }
    p::preview(ctx, kind, kind.into(), doc, revision, false).await
}

/// Gerencia as regras da comunidade.
#[poise::command(
    slash_command,
    guild_only,
    ephemeral,
    subcommands(
        "rules_preview",
        "rules_publish",
        "rules_update",
        "rules_list",
        "rules_add",
        "rules_edit",
        "rules_remove"
    ),
    default_member_permissions = "MANAGE_MESSAGES"
)]
pub async fn rules(_: Context<'_>) -> Result<(), Error> {
    Ok(())
}
/// Gerencia os links da comunidade.
#[poise::command(
    slash_command,
    guild_only,
    ephemeral,
    subcommands(
        "links_preview",
        "links_publish",
        "links_update",
        "links_list",
        "links_add",
        "links_edit",
        "links_remove"
    ),
    default_member_permissions = "MANAGE_MESSAGES"
)]
pub async fn links(_: Context<'_>) -> Result<(), Error> {
    Ok(())
}

/// preview ? rules.
#[poise::command(slash_command, guild_only, ephemeral, rename = "preview")]
async fn rules_preview(ctx: Context<'_>) -> Result<(), Error> {
    collection(ctx, "rules", "preview").await
}

/// publish ? rules.
#[poise::command(slash_command, guild_only, ephemeral, rename = "publish")]
async fn rules_publish(ctx: Context<'_>) -> Result<(), Error> {
    collection(ctx, "rules", "publish").await
}

/// update ? rules.
#[poise::command(slash_command, guild_only, ephemeral, rename = "update")]
async fn rules_update(ctx: Context<'_>) -> Result<(), Error> {
    collection(ctx, "rules", "update").await
}

/// list ? rules.
#[poise::command(slash_command, guild_only, ephemeral, rename = "list")]
async fn rules_list(ctx: Context<'_>) -> Result<(), Error> {
    collection(ctx, "rules", "list").await
}

/// add ? rules.
#[poise::command(slash_command, guild_only, ephemeral, rename = "add")]
async fn rules_add(
    ctx: Context<'_>,
    #[description = "Identificador ou n?mero da regra"] id: String,
) -> Result<(), Error> {
    change_entry(ctx, "rules", "add", id).await
}

/// edit ? rules.
#[poise::command(slash_command, guild_only, ephemeral, rename = "edit")]
async fn rules_edit(
    ctx: Context<'_>,
    #[description = "Identificador ou n?mero da regra"] id: String,
) -> Result<(), Error> {
    change_entry(ctx, "rules", "edit", id).await
}

/// remove ? rules.
#[poise::command(slash_command, guild_only, ephemeral, rename = "remove")]
async fn rules_remove(
    ctx: Context<'_>,
    #[description = "Identificador ou n?mero da regra"] id: String,
) -> Result<(), Error> {
    change_entry(ctx, "rules", "remove", id).await
}

/// preview ? links.
#[poise::command(slash_command, guild_only, ephemeral, rename = "preview")]
async fn links_preview(ctx: Context<'_>) -> Result<(), Error> {
    collection(ctx, "links", "preview").await
}

/// publish ? links.
#[poise::command(slash_command, guild_only, ephemeral, rename = "publish")]
async fn links_publish(ctx: Context<'_>) -> Result<(), Error> {
    collection(ctx, "links", "publish").await
}

/// update ? links.
#[poise::command(slash_command, guild_only, ephemeral, rename = "update")]
async fn links_update(ctx: Context<'_>) -> Result<(), Error> {
    collection(ctx, "links", "update").await
}

/// list ? links.
#[poise::command(slash_command, guild_only, ephemeral, rename = "list")]
async fn links_list(ctx: Context<'_>) -> Result<(), Error> {
    collection(ctx, "links", "list").await
}

/// add ? links.
#[poise::command(slash_command, guild_only, ephemeral, rename = "add")]
async fn links_add(
    ctx: Context<'_>,
    #[description = "Identificador ou n?mero da regra"] id: String,
) -> Result<(), Error> {
    change_entry(ctx, "links", "add", id).await
}

/// edit ? links.
#[poise::command(slash_command, guild_only, ephemeral, rename = "edit")]
async fn links_edit(
    ctx: Context<'_>,
    #[description = "Identificador ou n?mero da regra"] id: String,
) -> Result<(), Error> {
    change_entry(ctx, "links", "edit", id).await
}

/// remove ? links.
#[poise::command(slash_command, guild_only, ephemeral, rename = "remove")]
async fn links_remove(
    ctx: Context<'_>,
    #[description = "Identificador ou n?mero da regra"] id: String,
) -> Result<(), Error> {
    change_entry(ctx, "links", "remove", id).await
}
