//! Response lifecycle, route-safe diagnostics and at-most-once initial acknowledgements.
use super::{Context, Error};
use poise::serenity_prelude as s;
use std::{
    collections::HashMap,
    sync::{Mutex, atomic::Ordering},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stage {
    SlashDefer,
    ConfirmationAck,
    ConfirmationMessage,
    ConfirmationDisable,
    Prompt,
    ModalOpen,
    ModalSubmitAck,
    Progress,
    Result,
    ErrorNotice,
}
impl Stage {
    pub fn route(self) -> &'static str {
        match self {
            Self::SlashDefer | Self::ConfirmationAck | Self::ModalOpen | Self::ModalSubmitAck => {
                "POST /interactions/{id}/{token}/callback"
            }
            Self::ConfirmationMessage => {
                "GET /webhooks/{application_id}/{token}/messages/{message_id}"
            }
            Self::ConfirmationDisable => {
                "PATCH /webhooks/{application_id}/{token}/messages/{message_id}"
            }
            Self::Prompt | Self::Progress | Self::Result | Self::ErrorNotice => {
                "POST /webhooks/{application_id}/{token}"
            }
        }
    }
}
#[derive(Debug)]
pub struct Failure {
    pub stage: Stage,
    pub interaction: u64,
    pub status: Option<u16>,
    pub code: Option<isize>,
    pub age_ms: Option<u64>,
    pub duplicate: bool,
}
impl std::fmt::Display for Failure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let message = if self.duplicate {
            "Resposta inicial já tentada nesta instância; execução duplicada interrompida"
        } else if self.stage == Stage::Result {
            "O perfil já foi processado; falhou a resposta ao Discord. Confira o perfil antes de repetir"
        } else if self.stage == Stage::ErrorNotice {
            "Falhou a entrega do diagnóstico privado. Confira o erro original nos logs e o resultado antes de repetir"
        } else {
            match self.code {
                Some(10062 | 10015) => {
                    "Interação expirada ou não reconhecida pelo Discord. Reabra o comando ou a confirmação; esta execução foi interrompida antes de executar a alteração solicitada"
                }
                Some(40060) => {
                    "Interação já respondida. Esta execução foi interrompida; confira se há outra instância do bot ativa"
                }
                Some(50001) => {
                    "Discord negou acesso ao recurso (50001). Confira acesso ao canal e à guilda"
                }
                Some(50013) => {
                    "Discord recusou por permissão insuficiente (50013). Confira as permissões do bot"
                }
                _ => {
                    "Falha ao responder à interação; nenhuma resposta inicial será repetida automaticamente"
                }
            }
        };
        write!(f, "{message}. Etapa: {:?}", self.stage)?;
        if let Some(status) = self.status {
            write!(f, ". HTTP {status}")?;
        }
        if let Some(code) = self.code {
            write!(f, ", código Discord {code}")?;
        }
        Ok(())
    }
}
impl std::error::Error for Failure {}

#[derive(Default)]
pub struct Responses(Mutex<HashMap<u64, Instant>>);
impl Responses {
    fn claim(&self, id: u64, stage: Stage) -> Result<(), Failure> {
        let mut entries = self.0.lock().unwrap_or_else(|e| e.into_inner());
        entries.retain(|_, since| since.elapsed() < Duration::from_secs(1200));
        if entries.contains_key(&id) || entries.len() >= 8192 {
            return Err(Failure {
                stage,
                interaction: id,
                status: None,
                code: None,
                age_ms: age_ms(id),
                duplicate: true,
            });
        }
        entries.insert(id, Instant::now());
        Ok(())
    }
}
pub fn age_at(id: u64, now_ms: u64) -> Option<u64> {
    now_ms.checked_sub((id >> 22) + 1_420_070_400_000)
}
fn age_ms(id: u64) -> Option<u64> {
    age_at(
        id,
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .ok()?
            .as_millis()
            .try_into()
            .ok()?,
    )
}
pub fn tagged(stage: Stage, id: u64, error: s::Error) -> Error {
    let (status, code) = match &error {
        s::Error::Http(s::HttpError::UnsuccessfulRequest(r)) => {
            (Some(r.status_code.as_u16()), Some(r.error.code))
        }
        _ => (None, None),
    };
    // Do not retain the raw HTTP error: its URL contains an interaction token.
    Failure {
        stage,
        interaction: id,
        status,
        code,
        age_ms: age_ms(id),
        duplicate: false,
    }
    .into()
}

pub async fn initial(
    http: &s::Http,
    responses: &Responses,
    id: s::InteractionId,
    token: &str,
    stage: Stage,
    response: s::CreateInteractionResponse,
) -> Result<(), Error> {
    responses.claim(id.get(), stage)?;
    let age = age_ms(id.get());
    tracing::info!(process_id = std::process::id(), interaction = %id, ?stage, route = stage.route(), interaction_age_ms = age, initial_deadline_passed = age.is_some_and(|ms| ms >= 3000), "Enviando resposta inicial de interação");
    let started = Instant::now();
    http.create_interaction_response(id, token, &response, vec![])
        .await
        .map_err(|error| tagged(stage, id.get(), error))?;
    tracing::info!(interaction = %id, ?stage, route = stage.route(), request_ms = started.elapsed().as_millis() as u64, "Interação reconhecida pelo Discord");
    Ok(())
}
pub async fn defer(ctx: Context<'_>) -> Result<(), Error> {
    let poise::Context::Application(app) = ctx else {
        return Ok(());
    };
    if app.has_sent_initial_response.load(Ordering::SeqCst) {
        return Ok(());
    }
    initial(
        ctx.http(),
        &ctx.data().interactions,
        app.interaction.id,
        &app.interaction.token,
        Stage::SlashDefer,
        s::CreateInteractionResponse::Defer(
            s::CreateInteractionResponseMessage::new().ephemeral(true),
        ),
    )
    .await?;
    app.has_sent_initial_response.store(true, Ordering::SeqCst);
    Ok(())
}
pub async fn followup(
    ctx: Context<'_>,
    content: impl Into<String>,
    completed: bool,
) -> Result<(), Error> {
    let stage = if completed {
        Stage::Result
    } else {
        Stage::Progress
    };
    send(ctx, content.into(), stage).await
}
pub async fn notify_failure(ctx: Context<'_>, content: String) -> Result<(), Error> {
    send(ctx, content, Stage::ErrorNotice).await
}
async fn send(ctx: Context<'_>, content: String, stage: Stage) -> Result<(), Error> {
    if let poise::Context::Application(app) = ctx {
        return followup_request(
            ctx.http(),
            app.interaction.id,
            &app.interaction.token,
            stage,
            app.has_sent_initial_response.load(Ordering::SeqCst),
            content,
        )
        .await;
    }
    ctx.send(
        poise::CreateReply::default()
            .content(content)
            .ephemeral(true)
            .allowed_mentions(s::CreateAllowedMentions::new()),
    )
    .await
    .map_err(|error| tagged(stage, ctx.id(), error))?;
    Ok(())
}
pub async fn followup_request(
    http: &s::Http,
    id: s::InteractionId,
    token: &str,
    stage: Stage,
    acknowledged: bool,
    content: String,
) -> Result<(), Error> {
    anyhow::ensure!(
        acknowledged,
        "Resposta privada exige defer concluído; uma segunda resposta inicial não será tentada"
    );
    tracing::info!(interaction = %id, ?stage, route = stage.route(), "Enviando follow-up privado da interação");
    http.create_followup_message(
        token,
        &s::CreateInteractionResponseFollowup::new()
            .content(content)
            .ephemeral(true)
            .allowed_mentions(s::CreateAllowedMentions::new()),
        vec![],
    )
    .await
    .map_err(|e| tagged(stage, id.get(), e))?;
    Ok(())
}
pub async fn confirmation(
    http: &s::Http,
    responses: &Responses,
    id: s::InteractionId,
    token: &str,
    expected: &str,
    received: &str,
) -> Result<bool, Error> {
    initial(
        http,
        responses,
        id,
        token,
        Stage::ConfirmationAck,
        s::CreateInteractionResponse::Acknowledge,
    )
    .await?;
    Ok(expected == received)
}
pub fn may_report(ctx: Context<'_>, error: &Error) -> bool {
    if let poise::Context::Application(app) = ctx {
        return may_report_after(
            app.has_sent_initial_response.load(Ordering::SeqCst),
            ctx.id(),
            error,
        );
    }
    true
}
pub fn may_report_after(acknowledged: bool, parent: u64, error: &Error) -> bool {
    acknowledged
        && error
            .downcast_ref::<Failure>()
            .is_none_or(|failure| failure.interaction != parent)
}
pub async fn modal<M: poise::Modal>(
    ctx: Context<'_>,
    interaction: &s::ComponentInteraction,
    form: M,
    timeout: Duration,
) -> Result<Option<M>, Error> {
    let custom_id = format!("modal:{}", interaction.id);
    initial(
        ctx.http(),
        &ctx.data().interactions,
        interaction.id,
        &interaction.token,
        Stage::ModalOpen,
        M::create(Some(form), custom_id.clone()),
    )
    .await?;
    let author = ctx.author().id;
    let response = s::ModalInteractionCollector::new(&ctx.serenity_context().shard)
        .author_id(author)
        .filter(move |i| i.data.custom_id == custom_id)
        .timeout(timeout)
        .await;
    let Some(response) = response else {
        return Ok(None);
    };
    initial(
        ctx.http(),
        &ctx.data().interactions,
        response.id,
        &response.token,
        Stage::ModalSubmitAck,
        s::CreateInteractionResponse::Acknowledge,
    )
    .await?;
    M::parse(response.data.clone())
        .map(Some)
        .map_err(|_| anyhow::anyhow!("Formulário inválido; reabra o comando"))
}
