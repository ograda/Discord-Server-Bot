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
pub const ACTIVITY_UNAVAILABLE: &str =
    "Lembra o besta do Ograda que essa funcionalidade ainda não está disponível";

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
    (custom_id == INVITE_FRIENDS_ID).then(|| private_response(ACTIVITY_UNAVAILABLE.into()))
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

/// Same publication and guild checks as Copy; no administrative permission required.
pub fn resolve_activity(state: &State, origin: CopyOrigin) -> Result<()> {
    for invite in state.link_invites.values() {
        if resolve_copy(state, origin, &invite.custom_id).is_ok() {
            return Ok(());
        }
    }
    anyhow::bail!("Botão não corresponde à publicação de Links desta guilda")
}

pub fn activity_response(enabled: bool, valid_context: bool) -> s::CreateInteractionResponse {
    if !valid_context {
        s::CreateInteractionResponse::Message(private_response(
            "Activity disponível somente em uma publicação de Links válida nesta guilda.".into(),
        ))
    } else if enabled {
        s::CreateInteractionResponse::LaunchActivity
    } else {
        s::CreateInteractionResponse::Message(private_response(ACTIVITY_UNAVAILABLE.into()))
    }
}

/// Launch must be the INITIAL callback (type 12); never defer before this call.
/// Only a definitive rejection allows a fallback callback. Timeouts, 10062 and
/// 40060 are ambiguous/expired: do not retry or risk responding twice.
pub async fn respond_activity(
    http: &s::Http,
    responses: &crate::core::interactions::Responses,
    id: s::InteractionId,
    token: &str,
    enabled: bool,
    valid_context: bool,
) -> Result<()> {
    use super::components_v2::private_http;
    use crate::core::interactions::{self, Failure, Stage};
    let response = activity_response(enabled, valid_context);
    tracing::info!(module="links", action="invite_activity", stage="callback", interaction=%id, enabled, valid_context, "Respondendo ao botão de Activity");
    match private_http(interactions::initial(
        http,
        responses,
        id,
        token,
        Stage::ActivityLaunch,
        response,
    ))
    .await
    {
        Ok(()) => Ok(()),
        Err(error) => {
            let rejected = error.downcast_ref::<Failure>().is_some_and(|f| {
                enabled
                    && valid_context
                    && !f.duplicate
                    && matches!(f.status, Some(400 | 403 | 404))
                    && !matches!(f.code, Some(10062 | 40060 | 10015))
            });
            if !rejected {
                return Err(error);
            }
            tracing::warn!(module="links", action="invite_activity", stage="launch_rejected", cause=%error, "Lançamento recusado; enviando fallback privado após rejeição definitiva");
            private_http(http.create_interaction_response(
                id,
                token,
                &activity_response(false, true),
                vec![],
            ))
            .await
            .map_err(|e| interactions::tagged(Stage::ActivityLaunch, id.get(), e))?;
            Ok(())
        }
    }
}

/// Persistent Gateway handler, independent of administrative command collectors.
pub async fn handle_copy(
    ctx: &s::Context,
    interaction: &s::ComponentInteraction,
    data: &crate::core::Data,
) -> Result<()> {
    if interaction.data.custom_id == INVITE_FRIENDS_ID {
        let cfg = interaction
            .guild_id
            .and_then(|g| data.config.guilds.get(&g));
        let allowed =
            cfg.is_some_and(|cfg| cfg.command_enabled("links", data.config.global.modules));
        let enabled = allowed && cfg.is_some_and(|cfg| cfg.activities.invite.enabled);
        let mut valid = false;
        if allowed && let Some(guild) = interaction.guild_id {
            let origin = CopyOrigin {
                guild,
                message_guild: interaction.message.guild_id,
                channel: interaction.channel_id,
                message: interaction.message.id,
                author: interaction.message.author.id,
                bot: ctx.cache.current_user().id,
            };
            // Never queue behind a slow publication and miss the 3-second callback deadline.
            if let Ok(_guard) = data.store.gate.try_lock() {
                valid = data
                    .store
                    .read(guild)
                    .is_ok_and(|state| resolve_activity(&state, origin).is_ok());
            }
            // Private previews are short-lived, bot-authored and scoped to this interaction.
            let preview = interaction
                .message
                .flags
                .is_some_and(|f| f.contains(s::MessageFlags::EPHEMERAL))
                && origin.author == origin.bot
                && origin.message_guild.is_none_or(|g| g == guild)
                && interaction.message.channel_id == origin.channel;
            valid |= preview;
        }
        return respond_activity(
            &ctx.http,
            &data.interactions,
            interaction.id,
            &interaction.token,
            enabled,
            valid,
        )
        .await;
    }
    if !interaction.data.custom_id.starts_with("links-copy:") {
        return Ok(());
    }
    super::components_v2::private_http(interaction.create_response(
        &ctx.http,
        s::CreateInteractionResponse::Defer(
            s::CreateInteractionResponseMessage::new().ephemeral(true),
        ),
    ))
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
            cfg.command_enabled("links", data.config.global.modules),
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
    super::components_v2::private_http(
        interaction.edit_response(
            &ctx.http,
            s::EditInteractionResponse::new()
                .content(content)
                .allowed_mentions(s::CreateAllowedMentions::new()),
        ),
    )
    .await?;
    Ok(())
}
