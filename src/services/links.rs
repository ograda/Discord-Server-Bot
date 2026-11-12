use super::{
    catalog::{Board, LinksFile},
    content::{Document, Entry, discord_invite_url},
    storage::State,
};
use anyhow::{Context as _, Result, ensure};
use poise::serenity_prelude as s;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const ACTIONS: &[&str] = &["preview", "publish", "update", "reset", "add", "remove"];
pub const INVITE_FRIENDS_ID: &str = "links-invite-friends";

pub fn handles_invite_component(custom_id: &str) -> bool {
    custom_id == INVITE_FRIENDS_ID || custom_id.starts_with("links-copy:")
}

fn private_response(content: String) -> s::CreateInteractionResponseMessage {
    s::CreateInteractionResponseMessage::new()
        .content(content)
        .ephemeral(true)
        .allowed_mentions(s::CreateAllowedMentions::new())
}

pub fn invite_friends_response(custom_id: &str) -> Option<s::CreateInteractionResponseMessage> {
    (custom_id == INVITE_FRIENDS_ID).then(|| {
        private_response(
            "Lembra o besta do Ograda que essa funcionalidade ainda não está disponível".into(),
        )
    })
}

pub fn action_option(_: s::CreateCommandOption) -> s::CreateCommandOption {
    let mut option = s::CreateCommandOption::new(
        s::CommandOptionType::String,
        "ação",
        "Operação obrigatória do catálogo de Links",
    )
    .required(true)
    .set_autocomplete(false);
    for action in ACTIONS {
        option = option.add_string_choice(*action, *action);
    }
    option
}
pub fn removal_id<'a>(action: &str, id: Option<&'a str>) -> Option<&'a str> {
    if action == "remove" { id } else { None }
}
pub fn preferred_board(boards: &[Board]) -> Option<&Board> {
    if boards.len() == 1 {
        boards.first()
    } else {
        boards.iter().find(|b| b.id == "default")
    }
}
pub fn link_token(board: &str, id: &str) -> String {
    format!(
        "link:{:x}",
        Sha256::digest(format!("{board}:{id}").as_bytes())
    )
}
pub fn matching_links<'a>(file: &'a LinksFile, id: Option<&str>) -> Vec<(&'a Board, &'a Entry)> {
    file.boards
        .iter()
        .flat_map(|board| {
            board
                .links
                .iter()
                .filter(move |entry| {
                    id.is_none_or(|id| id == entry.id || id == link_token(&board.id, &entry.id))
                })
                .map(move |entry| (board, entry))
        })
        .collect()
}
pub fn autocomplete(file: &LinksFile, partial: &str) -> Vec<s::AutocompleteChoice> {
    matching_links(file, None)
        .into_iter()
        .filter(|(b, e)| {
            e.id.contains(partial)
                || e.title.to_lowercase().contains(&partial.to_lowercase())
                || b.title.to_lowercase().contains(&partial.to_lowercase())
        })
        .take(25)
        .map(|(b, e)| {
            s::AutocompleteChoice::new(
                format!("{} — {} ({})", e.title, e.id, b.id)
                    .chars()
                    .take(100)
                    .collect::<String>(),
                link_token(&b.id, &e.id),
            )
        })
        .collect()
}
pub fn with_guild_icon(mut doc: Document, icon: Option<String>) -> Document {
    doc.thumbnail = icon.unwrap_or_default();
    doc
}
pub fn copy_content(url: &str) -> Result<String> {
    ensure!(
        discord_invite_url(url),
        "Convite Discord configurado inválido"
    );
    let text = format!(
        "Convite da comunidade\n\n```\n{url}\n```\n\nCopie o endereço acima e compartilhe com seus amigos."
    );
    ensure!(
        text.encode_utf16().count() <= 2000,
        "Convite excede o limite da resposta privada"
    );
    Ok(text)
}
pub fn copy_response(url: &str) -> Result<s::CreateInteractionResponseMessage> {
    Ok(private_response(copy_content(url)?))
}
pub fn invite_components(doc: &Document, custom_id: &str) -> Result<Vec<s::CreateActionRow>> {
    let Some(invite) = doc.discord_invite()? else {
        return Ok(vec![]);
    };
    copy_content(&invite.url)?;
    ensure!(
        custom_id.len() <= 100,
        "Identificador do botão de convite excede o limite"
    );
    Ok(vec![s::CreateActionRow::Buttons(vec![
        s::CreateButton::new(custom_id)
            .label("📋 Copiar convite")
            .style(s::ButtonStyle::Secondary),
        s::CreateButton::new(INVITE_FRIENDS_ID)
            .label("🎮 Convidar amigos")
            .style(s::ButtonStyle::Secondary),
    ])])
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PublishedInvite {
    pub custom_id: String,
    pub url: String,
}
pub fn published_invite(doc: &Document, operation: u64) -> Result<Option<PublishedInvite>> {
    doc.discord_invite()?
        .map(|entry| {
            copy_content(&entry.url)?;
            Ok(PublishedInvite {
                custom_id: format!("links-copy:{operation}"),
                url: entry.url.clone(),
            })
        })
        .transpose()
}
#[derive(Clone, Copy)]
pub struct CopyOrigin {
    pub guild: s::GuildId,
    pub message_guild: Option<s::GuildId>,
    pub channel: s::ChannelId,
    pub message: s::MessageId,
    pub author: s::UserId,
    pub bot: s::UserId,
}
pub fn resolve_copy<'a>(state: &'a State, origin: CopyOrigin, custom_id: &str) -> Result<&'a str> {
    ensure!(
        origin.message_guild.is_none_or(|g| g == origin.guild) && origin.author == origin.bot,
        "Botão não pertence a uma publicação do bot nesta guilda"
    );
    let found = state
        .link_invites
        .iter()
        .find(|(_, invite)| invite.custom_id == custom_id);
    let Some((key, invite)) = found else {
        anyhow::bail!(
            "Este botão de convite está desatualizado; solicite Publish ao administrador"
        );
    };
    let publication = state
        .publications
        .get(key)
        .ok_or_else(|| anyhow::anyhow!("Publicação de Links não registrada"))?;
    ensure!(
        publication.channel == origin.channel
            && publication.message == origin.message
            && (key == "links" || key.starts_with("links:")),
        "Botão não corresponde à publicação gerenciada desta guilda"
    );
    copy_content(&invite.url)?;
    Ok(&invite.url)
}

/// Persistent Gateway handler, independent of administrative command collectors.
pub async fn handle_copy(
    ctx: &s::Context,
    interaction: &s::ComponentInteraction,
    data: &crate::core::Data,
) -> Result<()> {
    // Stateless routing survives restarts and is also used by private previews.
    if let Some(response) = invite_friends_response(&interaction.data.custom_id) {
        interaction
            .create_response(&ctx.http, s::CreateInteractionResponse::Message(response))
            .await?;
        return Ok(());
    }
    if !interaction.data.custom_id.starts_with("links-copy:") {
        return Ok(());
    }
    interaction
        .create_response(
            &ctx.http,
            s::CreateInteractionResponse::Defer(
                s::CreateInteractionResponseMessage::new().ephemeral(true),
            ),
        )
        .await?;
    let result = async {
        let guild = interaction
            .guild_id
            .ok_or_else(|| anyhow::anyhow!("Convite disponível somente nesta guilda"))?;
        let cfg = data
            .config
            .guilds
            .get(&guild)
            .ok_or_else(|| anyhow::anyhow!("Guilda não autorizada"))?;
        ensure!(
            cfg.enabled && cfg.modules.unwrap_or(data.config.global.modules).links,
            "Links desabilitado nesta guilda"
        );
        let _guard = data.store.gate.lock().await;
        let state = data
            .store
            .read(guild)
            .context("Falha ao ler estado operacional do convite")?;
        let bot = ctx.cache.current_user().id;
        let url = resolve_copy(
            &state,
            CopyOrigin {
                guild,
                message_guild: interaction.message.guild_id,
                channel: interaction.channel_id,
                message: interaction.message.id,
                author: interaction.message.author.id,
                bot,
            },
            &interaction.data.custom_id,
        )?;
        copy_content(url)
    }
    .await;
    let content = match result {
        Ok(content) => content,
        Err(error) => {
            tracing::warn!(guild = ?interaction.guild_id, module = "links", command = "links", action = "copy_invite", stage = "invite_copy", cause = %error, "Botão de convite inválido, desatualizado ou indisponível");
            "Convite indisponível ou botão desatualizado. Solicite ao administrador que confira o TOML e publique o quadro novamente.".into()
        }
    };
    interaction
        .edit_response(
            &ctx.http,
            s::EditInteractionResponse::new()
                .content(content)
                .allowed_mentions(s::CreateAllowedMentions::new()),
        )
        .await?;
    Ok(())
}
