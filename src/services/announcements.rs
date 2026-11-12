use super::{content::Document, storage::State};
use anyhow::{Result, bail, ensure};
use poise::serenity_prelude::{GuildId, Http, MessageId, PartialGuild};

pub mod media;

/// TOML/JSON identity fields are legacy values; the Discord guild supplies them at render time.
pub fn without_legacy_identity(mut doc: Document) -> Document {
    doc.thumbnail.clear();
    doc.footer.clear();
    doc
}

pub fn for_guild(base: &Document, guild: &PartialGuild) -> Result<Document> {
    let mut doc = without_legacy_identity(base.clone());
    doc.thumbnail = guild.icon_url().unwrap_or_default();
    doc.footer = format!("Comunicado oficial • {} • Ograda Solutions Bot", guild.name);
    ensure!(
        !doc.embeds("announcements")?.is_empty(),
        "Anúncio deve produzir um embed"
    );
    Ok(doc)
}

/// Fetch current guild identity for previews and each actual send/update, without bot-avatar fallbacks.
pub async fn build(http: &Http, guild: GuildId, base: &Document) -> Result<Document> {
    let current = guild.to_partial_guild(http).await?;
    for_guild(base, &current)
}

pub fn with_message(
    base: &super::content::Document,
    message: &str,
) -> Result<super::content::Document> {
    ensure!(
        !message.trim().is_empty(),
        "Mensagem do anúncio não pode ser vazia"
    );
    let mut doc = without_legacy_identity(base.clone());
    doc.text.clear();
    doc.description = message.into();
    ensure!(
        !doc.embeds("announcements")?.is_empty(),
        "Anúncio deve produzir um embed"
    );
    Ok(doc)
}
pub fn resolve_in_guild(state: &State, guild: GuildId, input: &str) -> Result<String> {
    if input.trim().starts_with("https://") {
        let url = url::Url::parse(input.trim())?;
        ensure!(
            matches!(
                url.host_str(),
                Some("discord.com" | "www.discord.com" | "canary.discord.com" | "ptb.discord.com")
            ) && url.query().is_none()
                && url.fragment().is_none(),
            "Use o link de uma mensagem Discord gerenciada"
        );
        let parts: Vec<_> = url.path().trim_matches('/').split('/').collect();
        ensure!(
            parts.len() == 4 && parts[0] == "channels" && parts[1] == guild.to_string(),
            "Link de mensagem não pertence à guilda atual"
        );
        let key = resolve(state, parts[3])?;
        ensure!(
            state.publications[&key].channel.to_string() == parts[2],
            "Canal do link não corresponde à publicação registrada"
        );
        return Ok(key);
    }
    resolve(state, input)
}
pub fn choices(state: &State, partial: &str) -> Vec<poise::serenity_prelude::AutocompleteChoice> {
    let mut keys: Vec<_> = state
        .publications
        .keys()
        .filter(|k| k.starts_with("announcement-"))
        .collect();
    keys.sort();
    keys.into_iter()
        .filter(|k| {
            k.contains(partial)
                || state.publications[*k].message.to_string().contains(partial)
                || summary(state, k)
                    .to_lowercase()
                    .contains(&partial.to_lowercase())
        })
        .take(25)
        .map(|key| {
            poise::serenity_prelude::AutocompleteChoice::new(
                format!("{} — {key}", summary(state, key))
                    .chars()
                    .take(100)
                    .collect::<String>(),
                key.clone(),
            )
        })
        .collect()
}

pub fn new_id(interaction: u64) -> String {
    format!("announcement-{interaction}")
}

/// Resolve only records of the current guild, never arbitrary Discord messages.
pub fn resolve(state: &State, input: &str) -> Result<String> {
    let input = input.trim();
    if input.starts_with("announcement-") {
        ensure!(
            state.publications.contains_key(input),
            "Anúncio não encontrado nesta guilda. Use a seleção ou o autocomplete de /announcement."
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
            "Informe a chave announcement-… ou um Message ID numérico associado à seleção de /announcement."
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
            "Message ID não associado a anúncio gerenciado nesta guilda. Use a seleção de /announcement."
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
