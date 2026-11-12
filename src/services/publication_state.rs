//! Durable publication journal. Positive evidence can reconcile a write; absence
//! in a bounded history search is NEVER evidence that a POST did not happen.
use super::{
    content::Document,
    storage::{Publication, State, Store},
};
use anyhow::{Context, Result, ensure};
use poise::serenity_prelude as s;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Debug)]
pub struct Blocked(pub String);
impl std::fmt::Display for Blocked {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for Blocked {}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Write {
    Create,
    Edit,
    Delete,
    Replace,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Pending {
    pub guild: s::GuildId,
    pub operation: u64,
    pub key: String,
    pub module: String,
    pub action: String,
    pub channel: s::ChannelId,
    pub write: Write,
    pub previous: Option<Publication>,
    pub confirmed_message: Option<s::MessageId>,
    pub rejected: Option<Rejection>,
    // Temporary draft only. Removed on completion; never used as TOML override.
    pub expected: Value,
    pub document: Option<Document>,
    pub invite: Option<super::links::PublishedInvite>,
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct Rejection {
    pub http_status: u16,
    pub discord_code: isize,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct History {
    pub pending: String,
    pub guild: s::GuildId,
    pub outcome: String,
    pub publication: Option<Publication>,
    pub rejection: Option<Rejection>,
    pub revision: u64,
}

pub fn rejection(error: &anyhow::Error) -> Option<Rejection> {
    match error.downcast_ref::<s::Error>()? {
        s::Error::Http(s::HttpError::UnsuccessfulRequest(r))
            if (400..500).contains(&r.status_code.as_u16())
                && ![408, 409, 425, 429].contains(&r.status_code.as_u16())
                && r.error.code > 0 =>
        {
            Some(Rejection {
                http_status: r.status_code.as_u16(),
                discord_code: r.error.code,
            })
        }
        _ => None,
    }
}
pub fn begin(store: &Store, state: &mut State, pending: Pending) -> Result<()> {
    ensure!(
        state.pending.is_none() && state.pending_operation.is_none(),
        "{}",
        describe(state)
    );
    state.pending = Some(format!("{}:{}", pending.key, pending.operation));
    let guild = pending.guild;
    state.pending_operation = Some(pending);
    store.write(guild, state)
}
pub fn received(
    store: &Store,
    state: &mut State,
    guild: s::GuildId,
    id: s::MessageId,
) -> Result<()> {
    let pending = state
        .pending_operation
        .as_mut()
        .context("Journal ausente após envio")?;
    ensure!(pending.guild == guild, "Journal pertence a outra guilda");
    pending.confirmed_message = Some(id);
    store.write(guild, state)
}
pub fn finish(
    state: &mut State,
    guild: s::GuildId,
    outcome: &str,
    publication: Option<Publication>,
    rejected: Option<Rejection>,
) -> Result<()> {
    if let Some(pending) = &state.pending_operation {
        ensure!(pending.guild == guild, "Journal pertence a outra guilda");
    }
    let label = state.pending.take().context("Registro pending ausente")?;
    state.publication_history.push(History {
        pending: label,
        guild,
        outcome: outcome.into(),
        publication,
        rejection: rejected,
        revision: state.revision,
    });
    state.pending_operation = None;
    state.revision += 1;
    Ok(())
}
/// Only an explicit Discord rejection is a proven failure. Timeouts, 5xx,
/// malformed success responses and disk errors remain pending across restarts.
pub fn failed(
    store: &Store,
    state: &mut State,
    guild: s::GuildId,
    error: anyhow::Error,
) -> anyhow::Error {
    let Some(rejected) = rejection(&error) else {
        return error.context(Blocked(describe(state)));
    };
    if let Some(pending) = &mut state.pending_operation {
        pending.rejected = Some(rejected);
    }
    if store.write(guild, state).is_err() {
        return error.context("Rejeição definitiva; falhou a gravação da evidência. Pending preservado no disco; confira logs antes de reconciliar");
    }
    let publication = state
        .pending_operation
        .as_ref()
        .and_then(|p| p.previous.clone());
    if let Err(storage_error) = finish(state, guild, "http_rejected", publication, Some(rejected))
        .and_then(|_| store.write(guild, state))
    {
        return error.context(format!(
            "Rejeição registrada, mas falhou a finalização do journal: {storage_error}"
        ));
    }
    error
}
pub fn describe(state: &State) -> String {
    if state.pending.is_none() && state.pending_operation.is_some() {
        return "Estado persistente inconsistente: journal sem identificador pending. Dados preservados; use reconciliação administrativa.".into();
    }
    let Some(label) = &state.pending else {
        return "Nenhuma publicação pendente.".into();
    };
    let (key, operation) = label
        .rsplit_once(':')
        .unwrap_or((label, "não identificado"));
    let module = if key == "links" || key.starts_with("links:") {
        "Links"
    } else if key.starts_with("announcement-") {
        "Announcements"
    } else {
        "Publicações"
    };
    let details = state.pending_operation.as_ref();
    let action = details.map_or("não registrada (journal legado)", |p| p.action.as_str());
    let channel = details
        .map(|p| p.channel)
        .or_else(|| state.publications.get(key).map(|p| p.channel));
    let message = details
        .and_then(|p| p.confirmed_message)
        .or_else(|| state.publications.get(key).map(|p| p.message));
    format!(
        "Existe uma publicação pendente.\nMódulo: {module}\nOperação: {action} ({operation})\nEstado: Resultado não confirmado\nCanal: {}\nMensagem: {}\nVerifique a publicação antes de tentar novamente. Use o procedimento reconcile-publication; ausência na busca não comprova falha.",
        channel.map_or("não registrado".into(), |id| id.to_string()),
        message.map_or("não confirmada".into(), |id| id.to_string())
    )
}

fn subset(expected: &Value, actual: &Value) -> bool {
    match (expected, actual) {
        (Value::Object(e), Value::Object(a)) => e.iter().all(|(key, value)| {
            if key == "id" {
                return true;
            }
            if value == &json!(false) && !a.contains_key(key) {
                return true;
            }
            a.get(key).is_some_and(|v| subset(value, v))
        }),
        (Value::Array(e), Value::Array(a)) => {
            e.len() == a.len() && e.iter().zip(a).all(|(e, a)| subset(e, a))
        }
        _ => expected == actual,
    }
}
pub fn matches(pending: &Pending, message: &Value, bot: s::UserId) -> bool {
    if message["author"]["id"] != json!(bot) || message["channel_id"] != json!(pending.channel) {
        return false;
    }
    if let Some(guild) = message.get("guild_id").filter(|g| !g.is_null())
        && guild != &json!(pending.guild)
    {
        return false;
    }
    let expected = &pending.expected;
    if pending.module == "links" {
        return message["flags"].as_u64().unwrap_or_default()
            & super::components_v2::IS_COMPONENTS_V2
            != 0
            && subset(&expected["components"], &message["components"]);
    }
    let mut expected = expected.clone();
    if let Some(image) = expected.pointer_mut("/embeds/0/image/url")
        && let Some(filename) = image
            .as_str()
            .and_then(super::announcements::media::attachment_filename)
    {
        let found = message["attachments"]
            .as_array()
            .and_then(|files| files.iter().find(|a| a["filename"] == filename));
        let Some(file) = found else {
            return false;
        };
        let Some(actual_image) = message
            .pointer("/embeds/0/image/url")
            .and_then(Value::as_str)
        else {
            return false;
        };
        let same_attachment = if actual_image == format!("attachment://{filename}") {
            true
        } else if let (Ok(a), Ok(b)) = (
            url::Url::parse(actual_image),
            url::Url::parse(file["url"].as_str().unwrap_or_default()),
        ) {
            a.scheme() == "https"
                && b.scheme() == "https"
                && ["cdn.discordapp.com", "media.discordapp.net"]
                    .contains(&a.host_str().unwrap_or_default())
                && ["cdn.discordapp.com", "media.discordapp.net"]
                    .contains(&b.host_str().unwrap_or_default())
                && a.path() == b.path()
        } else {
            false
        };
        if !same_attachment {
            return false;
        }
        *image = Value::String(actual_image.into());
    }
    message["content"].as_str().unwrap_or_default()
        == expected["content"].as_str().unwrap_or_default()
        && subset(&expected["embeds"], &message["embeds"])
}

pub fn apply_confirmed(state: &mut State, pending: &Pending, message: &Value) -> Result<()> {
    let id: s::MessageId = serde_json::from_value(message["id"].clone())?;
    if let Some(previous) = &pending.previous
        && previous.message != id
        && matches!(pending.write, Write::Create | Write::Replace)
    {
        state
            .archived_publications
            .push(super::storage::ArchivedPublication {
                key: pending.key.clone(),
                publication: previous.clone(),
                revision: state.revision,
            });
    }
    state.publications.insert(
        pending.key.clone(),
        Publication {
            channel: pending.channel,
            message: id,
        },
    );
    if pending.module == "links" {
        if let Some(invite) = &pending.invite {
            state
                .link_invites
                .insert(pending.key.clone(), invite.clone());
        } else {
            state.link_invites.remove(&pending.key);
        }
    } else if pending.module == "announcements" {
        let doc = pending
            .document
            .as_ref()
            .context("Rascunho do anúncio ausente; reconciliação manual necessária")?;
        let original: s::Message = serde_json::from_value(message.clone())?;
        if let Some(image) = super::announcements::media::published(doc, &original)? {
            state.announcement_images.insert(pending.key.clone(), image);
        } else {
            state.announcement_images.remove(&pending.key);
        }
        state.documents.insert(pending.key.clone(), doc.clone());
    }
    Ok(())
}

async fn fetch(http: &s::Http, channel: s::ChannelId, message: s::MessageId) -> Result<Value> {
    super::components_v2::request(
        http,
        s::http::Route::ChannelMessage {
            channel_id: channel,
            message_id: message,
        },
        s::http::LightMethod::Get,
        None::<&Value>,
    )
    .await
}
pub async fn inspect_message(
    state: &State,
    guild: s::GuildId,
    bot: s::UserId,
    http: &s::Http,
    id: s::MessageId,
) -> Result<Value> {
    let pending = state.pending_operation.as_ref().context("Journal legado sem rascunho/identificador: não é possível adotar uma mensagem automaticamente")?;
    ensure!(
        pending.guild == guild && pending.write != Write::Delete,
        "Operação/guilda não permite adoção de mensagem"
    );
    verify_channel(http, pending.channel, guild).await?;
    let message = fetch(http, pending.channel, id).await?;
    let identified = pending.confirmed_message == Some(id)
        || (pending.write == Write::Edit
            && pending.previous.as_ref().is_some_and(|p| p.message == id))
        || message["nonce"].as_str() == Some(&pending.operation.to_string())
        || message["nonce"].as_u64() == Some(pending.operation);
    ensure!(
        identified && message["id"] == json!(id) && matches(pending, &message, bot),
        "Mensagem não comprova o resultado desta operação; pending preservado"
    );
    Ok(message)
}
pub async fn inspect(
    state: &State,
    guild: s::GuildId,
    bot: s::UserId,
    http: &s::Http,
) -> Result<Option<Value>> {
    let Some(pending) = &state.pending_operation else {
        // Recover the known ID for diagnostics, but do not invent a missing action/expected payload.
        if let Some(label) = &state.pending
            && let Some((key, _)) = label.rsplit_once(':')
            && let Some(original) = state.publications.get(key)
        {
            verify_channel(http, original.channel, guild).await?;
            let message = fetch(http, original.channel, original.message).await?;
            super::components_v2::diagnose(guild, original.channel, &message, &[]);
        }
        return Ok(None);
    };
    ensure!(
        pending.guild == guild,
        "Journal de outra guilda; reconciliação recusada"
    );
    ensure!(
        pending.module == "links" || pending.module == "announcements",
        "Módulo não suportado por esta reconciliação"
    );
    ensure!(
        state.pending.as_deref() == Some(&format!("{}:{}", pending.key, pending.operation)),
        "Pending e journal inconsistentes; referências preservadas"
    );
    if pending.rejected.is_some() {
        ensure!(
            pending.confirmed_message.is_none(),
            "Journal contém recibo de sucesso e rejeição contraditórios; referências preservadas"
        );
        return Ok(Some(Value::Null));
    }
    verify_channel(http, pending.channel, guild).await?;
    let target = pending.confirmed_message.or_else(|| {
        (pending.write == Write::Edit || pending.write == Write::Delete)
            .then(|| pending.previous.as_ref().map(|p| p.message))
            .flatten()
    });
    if let Some(target) = target {
        match fetch(http, pending.channel, target).await {
            Ok(message)
                if pending.write != Write::Delete
                    && message["id"] == json!(target)
                    && matches(pending, &message, bot) =>
            {
                return Ok(Some(message));
            }
            Err(error)
                if pending.write == Write::Delete
                    && matches!(
                        rejection(&error),
                        Some(Rejection {
                            http_status: 404,
                            discord_code: 10008
                        })
                    ) =>
            {
                return Ok(Some(Value::Null));
            }
            Err(error) => return Err(error),
            _ => return Ok(None),
        }
    }
    // Positive nonce match in recent history can recover a POST without a saved response.
    let req = s::http::Request::new(
        s::http::Route::ChannelMessages {
            channel_id: pending.channel,
        },
        s::http::LightMethod::Get,
    )
    .params(Some(vec![("limit", "100".into())]));
    let messages: Vec<Value> = super::components_v2::private_http(http.fire(req)).await?;
    let candidates = messages
        .into_iter()
        .filter(|m| {
            m["nonce"].as_str() == Some(&pending.operation.to_string())
                || m["nonce"].as_u64() == Some(pending.operation)
        })
        .collect::<Vec<_>>();
    if candidates.len() == 1 && matches(pending, &candidates[0], bot) {
        Ok(candidates.into_iter().next())
    } else {
        Ok(None)
    }
}

async fn verify_channel(http: &s::Http, channel: s::ChannelId, guild: s::GuildId) -> Result<()> {
    let actual: Value = super::components_v2::request(
        http,
        s::http::Route::Channel {
            channel_id: channel,
        },
        s::http::LightMethod::Get,
        None::<&Value>,
    )
    .await?;
    ensure!(
        actual["id"] == json!(channel) && actual["guild_id"] == json!(guild),
        "Canal de outra guilda ou resposta inconsistente; reconciliação recusada"
    );
    Ok(())
}

/// Caller holds the Store gate and exclusive process lock; no Discord writes.
pub fn conclude(
    store: &Store,
    state: &mut State,
    guild: s::GuildId,
    evidence: Value,
) -> Result<()> {
    let pending = state
        .pending_operation
        .clone()
        .context("Journal legado não é conclusivo")?;
    ensure!(pending.guild == guild, "Guilda divergente");
    store.backup_publications(guild)?;
    let outcome = if pending.rejected.is_some() {
        "reconciled_rejection"
    } else if pending.write == Write::Delete {
        state.publications.remove(&pending.key);
        if pending.module == "announcements" {
            state.documents.remove(&pending.key);
            state.announcement_images.remove(&pending.key);
        }
        if pending.module == "links" {
            state.link_invites.remove(&pending.key);
        }
        "reconciled_deletion"
    } else {
        apply_confirmed(state, &pending, &evidence)?;
        "reconciled_success"
    };
    let publication = state.publications.get(&pending.key).cloned();
    finish(state, guild, outcome, publication, pending.rejected)?;
    store.write(guild, state)
}

pub async fn ensure_ready(
    store: &Store,
    guild: s::GuildId,
    bot: s::UserId,
    http: &s::Http,
) -> Result<()> {
    let _guard = store.gate.lock().await;
    let mut state = store.read(guild)?;
    if state.pending.is_none() && state.pending_operation.is_none() {
        return Ok(());
    }
    match inspect(&state, guild, bot, http).await {
        Ok(Some(evidence)) => conclude(store, &mut state, guild, evidence),
        Ok(None) => Err(Blocked(describe(&state)).into()),
        Err(error) => Err(error.context(Blocked(describe(&state)))),
    }
}

/// Offline administrator explicitly attests a definitive rejection from saved
/// logs. Does not infer failure from missing messages or discard the old record.
pub async fn attest_rejection(
    store: &Store,
    state: &mut State,
    guild: s::GuildId,
    bot: s::UserId,
    http: &s::Http,
    confirmation: &str,
    evidence: Rejection,
) -> Result<()> {
    ensure!(
        state.pending.as_deref() == Some(confirmation),
        "Confirmação não corresponde ao pending atual; nenhuma alteração"
    );
    ensure!(
        matches!(
            (evidence.http_status, evidence.discord_code),
            (403, 50080 | 50001 | 50013) | (400, 50035 | 50109 | 50068)
        ),
        "Evidência não é uma rejeição definitiva reconhecida; mantenha pending"
    );
    let (key, _) = confirmation
        .rsplit_once(':')
        .context("Identificação legada incompleta")?;
    ensure!(
        key == "links" || key.starts_with("links:") || key.starts_with("announcement-"),
        "Reconciliação restrita a Links/Announcements"
    );
    if let Some(pending) = &state.pending_operation {
        ensure!(
            pending.guild == guild && pending.confirmed_message.is_none(),
            "Há recibo de sucesso ou guilda divergente; rejeição manual recusada"
        );
        verify_channel(http, pending.channel, guild).await?;
        if let Some(previous) = &pending.previous {
            let original = fetch(http, previous.channel, previous.message).await?;
            ensure!(
                original["author"]["id"] == json!(bot)
                    && original["channel_id"] == json!(previous.channel),
                "Mensagem de outra autoria/canal"
            );
            ensure!(
                !matches(pending, &original, bot),
                "Mensagem já corresponde ao resultado esperado; use reconciliação de sucesso"
            );
        }
    } else {
        let previous = state.publications.get(key).context("Pending legado sem referência: falta identificação da requisição. Não conclua apenas pela ausência na busca")?;
        verify_channel(http, previous.channel, guild).await?;
        let original = fetch(http, previous.channel, previous.message).await?;
        ensure!(
            original["author"]["id"] == json!(bot)
                && original["channel_id"] == json!(previous.channel),
            "Mensagem de outra autoria/canal"
        );
        if key == "links" || key.starts_with("links:") {
            ensure!(
                evidence.discord_code != 50080
                    || original["flags"].as_u64().unwrap_or_default()
                        & super::components_v2::IS_COMPONENTS_V2
                        == 0,
                "Mensagem já convertida; a rejeição legada não é suficiente para concluir esta operação"
            );
        }
    }
    store.backup_publications(guild)?;
    let publication = state.publications.get(key).cloned();
    finish(
        state,
        guild,
        "operator_attested_rejection",
        publication,
        Some(evidence),
    )?;
    store.write(guild, state)
}
