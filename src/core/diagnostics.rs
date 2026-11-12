use super::Context;
use poise::serenity_prelude as s;

pub fn discord_route(method: &str, value: &str) -> &'static str {
    let Ok(url) = url::Url::parse(value) else {
        return "Discord HTTP (rota não identificada)";
    };
    let parts = url
        .path_segments()
        .map(|p| p.collect::<Vec<_>>())
        .unwrap_or_default();
    if parts.windows(1).any(|p| p == ["interactions"]) && parts.last() == Some(&"callback") {
        return "POST /interactions/{id}/{token}/callback";
    }
    if parts.contains(&"webhooks") {
        return match method {
            "PATCH" => "PATCH /webhooks/{application_id}/{token}/messages/{message_id}",
            "GET" => "GET /webhooks/{application_id}/{token}/messages/{message_id}",
            _ => "POST /webhooks/{application_id}/{token}",
        };
    }
    if parts.contains(&"members") {
        return if method == "PATCH" {
            "PATCH /guilds/{guild}/members/@me"
        } else {
            "GET /guilds/{guild}/members/{bot}"
        };
    }
    if parts.contains(&"guilds") {
        return "GET /guilds/{guild}";
    }
    "Discord HTTP (rota omitida)"
}
pub fn http_message(status: u16, code: isize) -> String {
    match code {
        10062 => format!(
            "Interação expirada ou não reconhecida (HTTP {status}, código 10062). Reabra o comando ou a confirmação. Esse código não indica falta de permissão"
        ),
        10015 => format!(
            "Resposta da interação indisponível ou expirada (HTTP {status}, código 10015). Confira o resultado antes de repetir"
        ),
        40060 => format!(
            "Interação já respondida (HTTP {status}, código 40060). Verifique se há instâncias simultâneas; não repita a resposta inicial"
        ),
        50001 => format!(
            "Discord negou acesso ao recurso (HTTP {status}, código 50001). Confira acesso ao canal e à guilda"
        ),
        50013 => format!(
            "Discord recusou por permissão insuficiente (HTTP {status}, código 50013). Confira as permissões do bot"
        ),
        _ => format!(
            "Discord recusou a operação: HTTP {status}, código {code}. Confira o resultado e o registro pending, quando aplicável, antes de repetir"
        ),
    }
}

pub fn action(ctx: Context<'_>) -> String {
    let command = ctx.command();
    if let poise::Context::Application(app) = ctx
        && let Some(arg) = app
            .args
            .iter()
            .find(|a| a.name == "ação" || a.name == "modo")
    {
        if let s::ResolvedValue::String(value) = arg.value
            && command
                .parameters
                .iter()
                .find(|p| p.name == arg.name)
                .is_some_and(|p| p.choices.iter().any(|c| c.name == value))
        {
            return value.into();
        }
        if let s::ResolvedValue::Integer(index) = arg.value
            && let Some(choice) = usize::try_from(index).ok().and_then(|i| {
                command
                    .parameters
                    .iter()
                    .find(|p| p.name == arg.name)
                    .and_then(|p| p.choices.get(i))
            })
        {
            return choice.name.clone();
        }
        return "escolha_invalida".into();
    }
    if command.name == "add" || command.name == "remove" {
        return command.name.clone();
    }
    crate::config::availability::default_action(&command.name)
        .unwrap_or("send")
        .into()
}
pub fn failure(ctx: Context<'_>, error: &anyhow::Error) -> String {
    let interaction = error.downcast_ref::<super::interactions::Failure>();
    let action = action(ctx);
    let module = ctx
        .command()
        .qualified_name
        .split(' ')
        .next()
        .unwrap_or("unknown")
        .to_string();
    let http = interaction.and_then(|i| i.status.zip(i.code)).or_else(|| {
        error.downcast_ref::<s::Error>().and_then(|e| match e {
            s::Error::Http(s::HttpError::UnsuccessfulRequest(r)) => {
                Some((r.status_code.as_u16(), r.error.code))
            }
            _ => None,
        })
    });
    let message = if let Some(interaction) = interaction {
        interaction.to_string()
    } else if let Some((status, code)) = http {
        http_message(status, code)
    } else if error.downcast_ref::<s::Error>().is_some() {
        "Falha de transporte Discord; resultado pode ser incerto. Confira a publicação e o registro pending antes de repetir.".into()
    } else if error.downcast_ref::<std::io::Error>().is_some() {
        "Falha no armazenamento; confira permissões do volume e os backups antes de repetir.".into()
    } else {
        error.to_string()
    };
    let lower = message.to_lowercase();
    let route = interaction.map(|i| i.stage.route()).or_else(|| {
        error.downcast_ref::<s::Error>().and_then(|e| match e {
            s::Error::Http(s::HttpError::UnsuccessfulRequest(r)) => {
                Some(discord_route(r.method.as_str(), &r.url))
            }
            _ => None,
        })
    });
    let stage = if interaction.is_some() {
        "interaction_response"
    } else if lower.contains("download") {
        "download"
    } else if lower.contains("processamento") {
        "image_processing"
    } else if lower.contains("http") || http.is_some() {
        "http"
    } else if lower.contains("toml") || lower.contains("armazenamento") {
        "configuration"
    } else {
        "validation_or_authorization"
    };
    // Use only the sanitized administrator message; never Debug arbitrary HTTP errors or arguments.
    tracing::warn!(process_id = std::process::id(), guild = ?ctx.guild_id(), module, command = ctx.command().qualified_name, action, stage, response_stage = ?interaction.map(|i| i.stage), route, interaction = interaction.map(|i| i.interaction), interaction_age_ms = interaction.and_then(|i| i.age_ms), cause = message, http_status = http.map(|v| v.0), discord_code = http.map(|v| v.1), "Falha em operação administrativa");
    message
}
