//! Links-only Components V2 adapter for Serenity 0.12.5. Reuses its HTTP client,
//! authentication and rate limiter; never changes the other modules' payloads.
use super::content::{Document, valid_url};
use anyhow::{Result, ensure};
use poise::serenity_prelude as s;
use serde::{Deserialize, Serialize, Serializer};
use serde_json::{Value, json};
use tracing::instrument::WithSubscriber;

pub const IS_COMPONENTS_V2: u64 = 1 << 15;
pub const TEXT_LIMIT: usize = 4000;
pub const COMPONENT_LIMIT: usize = 40;

/// Serenity's instrumented Request Debug includes webhook tokens and message
/// bodies. Suppress those third-party spans, retaining our route-safe logs.
pub(crate) async fn private_http<T>(future: impl std::future::Future<Output = T>) -> T {
    future
        .with_subscriber(tracing::subscriber::NoSubscriber::default())
        .await
}

#[derive(Clone, Debug)]
pub enum Component {
    Container {
        color: u32,
        children: Vec<Component>,
    },
    Text(String),
    Section {
        text: String,
        thumbnail: String,
    },
    Separator,
    Row(Vec<s::CreateButton>),
    Gallery(String),
}
impl Serialize for Component {
    fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        let value = match self {
            Self::Container { color, children } => {
                json!({"type":17,"accent_color":color,"components":children})
            }
            Self::Text(content) => json!({"type":10,"content":content}),
            Self::Section { text, thumbnail } => {
                json!({"type":9,"components":[{"type":10,"content":text}],"accessory":{"type":11,"media":{"url":thumbnail}}})
            }
            Self::Separator => json!({"type":14,"divider":true,"spacing":1}),
            Self::Row(buttons) => json!({"type":1,"components":buttons}),
            Self::Gallery(url) => json!({"type":12,"items":[{"media":{"url":url}}]}),
        };
        value.serialize(serializer)
    }
}
impl Component {
    fn count(&self) -> usize {
        match self {
            Self::Container { children, .. } => 1 + children.iter().map(Self::count).sum::<usize>(),
            Self::Section { .. } => 3,
            Self::Row(buttons) => 1 + buttons.len(),
            _ => 1,
        }
    }
    fn text_len(&self) -> usize {
        match self {
            Self::Container { children, .. } => children.iter().map(Self::text_len).sum(),
            Self::Text(text) | Self::Section { text, .. } => text.encode_utf16().count(),
            _ => 0,
        }
    }
}

/// A single Container shared by Preview and Publish. Content remains TOML-owned.
pub fn build(doc: &Document, copy_id: &str, controls: Vec<s::CreateButton>) -> Result<Message> {
    ensure!(doc.color <= 0xffffff, "Cor do quadro fora do intervalo RGB");
    ensure!(
        controls.len() <= 5,
        "Limite de cinco botões administrativos"
    );
    doc.discord_invite()?; // Reject ambiguous principal invites before sorting.
    let mut ids = std::collections::HashSet::new();
    for entry in &doc.entries {
        entry.validate_link()?;
        ensure!(ids.insert(&entry.id), "Link ID duplicado no quadro");
    }
    let mut children = vec![];
    let heading = [
        (!doc.title.is_empty()).then(|| format!("## {}", doc.title)),
        (!doc.description.is_empty()).then(|| doc.description.clone()),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>()
    .join("\n\n");
    if !heading.is_empty() {
        if doc.thumbnail.is_empty() {
            children.push(Component::Text(heading));
        } else {
            valid_url(&doc.thumbnail)?;
            children.push(Component::Section {
                text: heading,
                thumbnail: doc.thumbnail.clone(),
            });
        }
    }
    if !doc.text.is_empty() {
        children.push(Component::Text(doc.text.clone()));
    }
    // Stable sort: preserve each regular entry's configured order and keep invite last.
    let mut entries = doc.entries.iter().collect::<Vec<_>>();
    entries.sort_by_key(|e| (e.is_discord_invite().unwrap_or(false), e.order));
    let mut blocks = vec![];
    for entry in entries {
        let title = if entry.emoji.is_empty() {
            entry.title.clone()
        } else {
            format!("{} {}", entry.emoji, entry.title)
        };
        let label = if entry.is_discord_invite()? {
            "Link do convite"
        } else {
            entry.label.as_deref().unwrap_or("Abrir link")
        };
        let mut block = format!("**{title} — [{label}](<{}>)**", entry.url);
        if !entry.description.is_empty() {
            block.push('\n');
            block.push_str(&entry.description);
        }
        blocks.push(block);
    }
    if !blocks.is_empty() {
        children.push(Component::Text(blocks.join("\n\n")));
    }
    if !doc.image.is_empty() {
        valid_url(&doc.image)?;
        children.push(Component::Gallery(doc.image.clone()));
    }
    if !doc.footer.is_empty() {
        children.push(Component::Text(doc.footer.clone()));
    }
    for row in super::links::invite_components(doc, copy_id)? {
        if let s::CreateActionRow::Buttons(buttons) = row {
            children.push(Component::Separator);
            children.push(Component::Row(buttons));
        }
    }
    if !controls.is_empty() {
        children.push(Component::Separator);
        children.push(Component::Row(controls));
    }
    ensure!(!children.is_empty(), "Quadro de Links vazio");
    let root = Component::Container {
        color: doc.color,
        children,
    };
    ensure!(
        root.text_len() <= TEXT_LIMIT,
        "Links excede {TEXT_LIMIT} caracteres de texto em Components V2; conteúdo não truncado"
    );
    ensure!(
        root.count() <= COMPONENT_LIMIT,
        "Links excede {COMPONENT_LIMIT} componentes"
    );
    Ok(Message {
        flags: IS_COMPONENTS_V2,
        components: vec![root],
        allowed_mentions: s::CreateAllowedMentions::new(),
    })
}

#[derive(Clone, Debug, Serialize)]
pub struct Message {
    pub flags: u64,
    pub components: Vec<Component>,
    allowed_mentions: s::CreateAllowedMentions,
}
impl Message {
    pub fn ephemeral(mut self) -> Self {
        self.flags |= 64;
        self
    }
    pub fn notice(mut self, notice: &str) -> Result<Self> {
        if let Component::Container { children, .. } = &mut self.components[0] {
            children.insert(0, Component::Text(notice.into()));
        }
        ensure!(
            self.components[0].text_len() <= TEXT_LIMIT,
            "Prévia excede {TEXT_LIMIT} caracteres incluindo controles; conteúdo não truncado"
        );
        ensure!(
            self.components[0].count() <= COMPONENT_LIMIT,
            "Prévia excede o limite de componentes"
        );
        Ok(self)
    }
    /// Discord explicitly supports in-place migration with the legacy fields cleared.
    /// This flag cannot be removed later. Attachments are retained, never discarded.
    pub fn edit_payload(&self, original: &RemoteMessage) -> Value {
        let mut payload = serde_json::to_value(self).expect("typed Components V2 payload");
        payload["flags"] = json!(IS_COMPONENTS_V2 | (original.flags & 4));
        payload["content"] = Value::Null;
        payload["embeds"] = json!([]);
        payload["sticker_ids"] = json!([]);
        payload["poll"] = Value::Null;
        payload["attachments"] = json!(
            original
                .attachments
                .iter()
                .map(|a| json!({"id":a.id,"filename":a.filename}))
                .collect::<Vec<_>>()
        );
        payload
    }
}

#[derive(Debug, Deserialize)]
pub struct RemoteMessage {
    pub id: s::MessageId,
    pub channel_id: s::ChannelId,
    pub author: s::User,
    #[serde(default)]
    pub guild_id: Option<s::GuildId>,
    #[serde(default, deserialize_with = "flags_or_zero")]
    pub flags: u64,
    #[serde(default)]
    pub attachments: Vec<s::Attachment>,
}
fn flags_or_zero<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> std::result::Result<u64, D::Error> {
    Ok(Option::<u64>::deserialize(deserializer)?.unwrap_or_default())
}
impl RemoteMessage {
    pub fn is_v2(&self) -> bool {
        self.flags & IS_COMPONENTS_V2 != 0
    }
}

pub async fn request<T: serde::de::DeserializeOwned>(
    http: &s::Http,
    route: s::http::Route<'_>,
    method: s::http::LightMethod,
    body: Option<&impl Serialize>,
) -> Result<T> {
    let stage = match &route {
        s::http::Route::ChannelMessage { .. } => "managed_message",
        s::http::Route::ChannelMessages { .. } => "create_message",
        s::http::Route::WebhookFollowupMessages { .. } => "private_preview",
        s::http::Route::WebhookFollowupMessage { .. } => "close_preview",
        _ => "components_v2",
    };
    let body = body.map(serde_json::to_vec).transpose()?;
    let request = s::http::Request::new(route, method).body(body);
    match private_http(http.fire(request)).await {
        Ok(value) => Ok(value),
        Err(error) => {
            let (status, code) = match &error {
                s::Error::Http(s::HttpError::UnsuccessfulRequest(r)) => {
                    (Some(r.status_code.as_u16()), Some(r.error.code))
                }
                _ => (None, None),
            };
            tracing::warn!(
                module = "links",
                stage,
                http_status = status,
                discord_code = code,
                "Falha no transporte Components V2; payload e URL omitidos"
            );
            Err(error.into())
        }
    }
}

pub async fn get(
    http: &s::Http,
    channel: s::ChannelId,
    message: s::MessageId,
) -> Result<RemoteMessage> {
    request(
        http,
        s::http::Route::ChannelMessage {
            channel_id: channel,
            message_id: message,
        },
        s::http::LightMethod::Get,
        None::<&Message>,
    )
    .await
}
pub async fn create(
    http: &s::Http,
    channel: s::ChannelId,
    payload: &Message,
) -> Result<RemoteMessage> {
    request(
        http,
        s::http::Route::ChannelMessages {
            channel_id: channel,
        },
        s::http::LightMethod::Post,
        Some(payload),
    )
    .await
}
pub async fn edit(
    http: &s::Http,
    channel: s::ChannelId,
    original: &RemoteMessage,
    payload: &Message,
) -> Result<RemoteMessage> {
    request(
        http,
        s::http::Route::ChannelMessage {
            channel_id: channel,
            message_id: original.id,
        },
        s::http::LightMethod::Patch,
        Some(&payload.edit_payload(original)),
    )
    .await
}

/// Uses the same optimistic revisions and pending journal as the V1 publisher.
/// Migration is a single PATCH, never DELETE + POST. A failed PATCH preserves IDs.
pub async fn publish(
    store: &super::storage::Store,
    catalog: &super::catalog::Catalog,
    guild: s::GuildId,
    bot: s::UserId,
    http: &s::Http,
    plan: &super::publishing::Plan,
) -> Result<String> {
    use super::storage::{ArchivedPublication, Publication};
    use anyhow::Context as _;
    let invite = super::links::published_invite(&plan.document, plan.operation)?;
    let payload = build(
        &plan.document,
        invite.as_ref().map_or("", |i| i.custom_id.as_str()),
        vec![],
    )?;
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
    let mut original = None;
    let mut missing = false;
    if let Some(publication) = &existing {
        ensure!(
            publication.channel == plan.channel,
            "Canal configurado mudou; referências preservadas, reconcilie o registro antes de continuar"
        );
        match get(http, plan.channel, publication.message).await {
            Ok(message) => {
                ensure!(
                    message.author.id == bot
                        && message.channel_id == plan.channel
                        && message.guild_id.is_none_or(|g| g == guild),
                    "Mensagem registrada não pertence ao bot nesta guilda/canal"
                );
                if !message.is_v2() && !plan.delete {
                    tracing::info!(%guild, module="links", stage="migrate_components_v2", message=%message.id, "Convertendo publicação V1 confirmada por PATCH; Message ID preservado, flag irreversível");
                }
                original = Some(message);
            }
            Err(error) => {
                let missing_message = matches!(error.downcast_ref::<s::Error>(), Some(s::Error::Http(s::HttpError::UnsuccessfulRequest(r))) if r.status_code.as_u16() == 404 && r.error.code == 10008);
                if !missing_message || (!plan.delete && plan.action != "publish") {
                    return Err(error);
                }
                missing = true;
                if !plan.delete {
                    state.archived_publications.push(ArchivedPublication {
                        key: plan.key.clone(),
                        publication: publication.clone(),
                        revision: state.revision,
                    });
                }
            }
        }
    }
    state.pending = Some(format!("{}:{}", plan.key, plan.operation));
    store.write(guild, &state)?;
    if plan.delete {
        if let Some(original) = original {
            match plan.channel.delete_message(http, original.id).await {
                Ok(()) => (),
                Err(s::Error::Http(s::HttpError::UnsuccessfulRequest(r)))
                    if r.status_code.as_u16() == 404 && r.error.code == 10008 => {}
                Err(error) => return Err(error.into()),
            }
        }
        state.publications.remove(&plan.key);
        state.link_invites.remove(&plan.key);
    } else {
        let message = match original {
            Some(original) => edit(http, plan.channel, &original, &payload).await?,
            None => create(http, plan.channel, &payload).await?,
        };
        ensure!(
            message.channel_id == plan.channel && message.author.id == bot,
            "Resposta de publicação inesperada; operação pendente preservada"
        );
        if let Some(publication) = &existing {
            ensure!(
                missing || publication.message == message.id,
                "Discord retornou outro Message ID ao editar; referências preservadas"
            );
        }
        state.publications.insert(
            plan.key.clone(),
            Publication {
                channel: plan.channel,
                message: message.id,
            },
        );
        if let Some(invite) = invite {
            state.link_invites.insert(plan.key.clone(), invite);
        } else {
            state.link_invites.remove(&plan.key);
        }
    }
    state.pending = None;
    state.revision += 1;
    store.write(guild, &state)?;
    if plan.delete {
        return Ok(format!(
            "Publicação `{}` removida; definição TOML preservada.",
            plan.key
        ));
    }
    let publication = state
        .publications
        .get(&plan.key)
        .context("Publicação sem registro")?;
    let recovery = if missing {
        "A mensagem anterior foi apagada. Uma nova publicação foi criada e o ID anterior foi arquivado.\n"
    } else {
        ""
    };
    Ok(format!(
        "{recovery}Links publicado com Components V2; estado salvo nesta guilda. Identificador: `{}`\nMessage ID: `{}`\n[Ver mensagem]({})",
        plan.key,
        publication.message,
        super::announcements::message_link(guild, publication)
    ))
}

pub async fn preview(ctx: crate::core::Context<'_>, payload: &Message) -> Result<RemoteMessage> {
    use anyhow::Context as _;
    crate::core::interactions::defer(ctx).await?;
    let poise::Context::Application(app) = ctx else {
        anyhow::bail!("Prévia exige slash command");
    };
    request(
        ctx.http(),
        s::http::Route::WebhookFollowupMessages {
            application_id: app.interaction.application_id,
            token: &app.interaction.token,
        },
        s::http::LightMethod::Post,
        Some(&payload.clone().ephemeral()),
    )
    .await
    .map_err(|error| {
        safe_interaction_error(
            error,
            app.interaction.id,
            crate::core::interactions::Stage::Prompt,
        )
    })
    .context("Falha na prévia Components V2")
}
pub async fn close_preview(
    ctx: crate::core::Context<'_>,
    original: &RemoteMessage,
    doc: &Document,
) -> Result<()> {
    let poise::Context::Application(app) = ctx else {
        anyhow::bail!("Prévia exige slash command");
    };
    // V2 messages cannot be emptied: replace controls with the original static layout.
    let mut layout = build(doc, "links-preview-expired", vec![])?;
    if let Component::Container { children, .. } = &mut layout.components[0] {
        for child in children.iter_mut() {
            if let Component::Row(buttons) = child {
                for button in buttons {
                    *button = button.clone().disabled(true);
                }
            }
        }
    }
    let mut body = serde_json::to_value(layout)?;
    body["flags"] = json!(IS_COMPONENTS_V2 | 64);
    let _: RemoteMessage = request(
        ctx.http(),
        s::http::Route::WebhookFollowupMessage {
            application_id: app.interaction.application_id,
            token: &app.interaction.token,
            message_id: original.id,
        },
        s::http::LightMethod::Patch,
        Some(&body),
    )
    .await
    .map_err(|error| {
        safe_interaction_error(
            error,
            app.interaction.id,
            crate::core::interactions::Stage::ConfirmationDisable,
        )
    })?;
    Ok(())
}
fn safe_interaction_error(
    error: anyhow::Error,
    id: s::InteractionId,
    stage: crate::core::interactions::Stage,
) -> anyhow::Error {
    match error.downcast::<s::Error>() {
        Ok(error) => crate::core::interactions::tagged(stage, id.get(), error),
        Err(_) => anyhow::anyhow!("Falha local no payload Components V2; etapa {stage:?}"),
    }
}
