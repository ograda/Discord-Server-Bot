use super::storage::State;
use anyhow::{Result, bail, ensure};
use poise::serenity_prelude::{GuildId, MessageId};

pub fn new_id(interaction: u64) -> String {
    format!("announcement-{interaction}")
}

/// Resolve only records of the current guild, never arbitrary Discord messages.
pub fn resolve(state: &State, input: &str) -> Result<String> {
    let input = input.trim();
    if input.starts_with("announcement-") {
        ensure!(
            state.publications.contains_key(input),
            "Anúncio não encontrado nesta guilda. Use /announcements list ou o autocomplete."
        );
        ensure!(
            state.documents.contains_key(input),
            "Registro do anúncio está incompleto; conteúdo persistido ausente."
        );
        return Ok(input.into());
    }
    let id = input
        .parse::<u64>()
        .ok()
        .filter(|id| *id != 0)
        .map(MessageId::new);
    let Some(id) = id else {
        bail!(
            "Informe a chave announcement-… ou um Message ID numérico listado por /announcements list."
        );
    };
    let matches: Vec<_> = state
        .publications
        .iter()
        .filter(|(key, value)| key.starts_with("announcement-") && value.message == id)
        .collect();
    ensure!(
        matches.len() <= 1,
        "Message ID associado a múltiplos registros; reconciliação necessária"
    );
    let Some((key, _)) = matches.first() else {
        bail!(
            "Message ID não associado a anúncio gerenciado nesta guilda. Use /announcements list."
        );
    };
    ensure!(
        state.documents.contains_key(*key),
        "Registro incompleto: conteúdo persistido ausente"
    );
    Ok((*key).clone())
}
pub fn summary(state: &State, key: &str) -> String {
    state
        .documents
        .get(key)
        .map(|d| {
            if d.title.is_empty() {
                &d.text
            } else {
                &d.title
            }
        })
        .map(|s| s.chars().take(120).collect())
        .unwrap_or_else(|| "Conteúdo ausente".into())
}
pub fn message_link(guild: GuildId, publication: &super::storage::Publication) -> String {
    format!(
        "https://discord.com/channels/{guild}/{}/{}",
        publication.channel, publication.message
    )
}
