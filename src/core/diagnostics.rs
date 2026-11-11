use super::Context;
use poise::serenity_prelude as s;

pub fn action(ctx: Context<'_>) -> String {
    let command = ctx.command();
    if let poise::Context::Application(app) = ctx
        && let Some(arg) = app
            .args
            .iter()
            .find(|a| a.name == "ação" || a.name == "modo")
    {
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
    let action = action(ctx);
    let module = ctx
        .command()
        .qualified_name
        .split(' ')
        .next()
        .unwrap_or("unknown")
        .to_string();
    let http = error.downcast_ref::<s::Error>().and_then(|e| match e {
        s::Error::Http(s::HttpError::UnsuccessfulRequest(r)) => {
            Some((r.status_code.as_u16(), r.error.code))
        }
        _ => None,
    });
    let message = if let Some((status, code)) = http {
        format!(
            "Discord recusou a operação: HTTP {status}, código {code}. Confira permissões, acesso e o registro pending antes de repetir."
        )
    } else if error.downcast_ref::<s::Error>().is_some() {
        "Falha de transporte Discord; resultado pode ser incerto. Confira a publicação e o registro pending antes de repetir.".into()
    } else if error.downcast_ref::<std::io::Error>().is_some() {
        "Falha no armazenamento; confira permissões do volume e os backups antes de repetir.".into()
    } else {
        error.to_string()
    };
    let lower = message.to_lowercase();
    let stage = if lower.contains("download") {
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
    tracing::warn!(guild = ?ctx.guild_id(), module, command = ctx.command().qualified_name, action, stage, cause = message, http_status = http.map(|v| v.0), "Falha em operação administrativa");
    message
}
