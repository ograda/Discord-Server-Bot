//! Administrative state reconciliation, never sends/edits/deletes Discord messages.
use anyhow::{Context, Result, ensure};
use discord_server_bot::{
    config,
    services::{publication_state as journal, storage::Store},
};
use poise::serenity_prelude as s;
use std::path::Path;

#[tokio::main]
async fn main() -> Result<()> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args.is_empty() || args.iter().any(|a| a == "--help") {
        println!(
            "reconcile-publication --guild ID [--apply] [--reject-http 403 --reject-code 50080 --confirm-pending CHAVE:OPERACAO]\nPadrão: diagnóstico sem gravação. Pare o bot para adquirir o lock exclusivo. Rejeição manual exige evidência nos logs; nunca use ausência em busca como prova de falha."
        );
        return Ok(());
    }
    // Show only the application's sanitized metadata, never third-party HTTP
    // debug spans containing webhook tokens or private bodies.
    tracing_subscriber::fmt()
        .with_env_filter("discord_server_bot=info")
        .init();
    let value = |key: &str| {
        args.iter()
            .position(|a| a == key)
            .and_then(|i| args.get(i + 1))
            .filter(|v| !v.starts_with("--"))
            .map(String::as_str)
    };
    for arg in args.iter().filter(|a| a.starts_with("--")) {
        ensure!(
            [
                "--guild",
                "--apply",
                "--reject-http",
                "--reject-code",
                "--confirm-pending",
                "--message-id"
            ]
            .contains(&arg.as_str()),
            "Opção desconhecida: {arg}"
        );
    }
    match dotenvy::dotenv() {
        Ok(_) => (),
        Err(e) if e.not_found() => (),
        Err(_) => anyhow::bail!(".env inválido"),
    }
    let guild = s::GuildId::new(value("--guild").context("Informe --guild")?.parse()?);
    let config = config::load(Path::new(
        &std::env::var("CONFIG_DIR").unwrap_or_else(|_| "config".into()),
    ))?;
    let cfg = config
        .guilds
        .get(&guild)
        .filter(|g| g.enabled)
        .context("Guilda não autorizada/habilitada")?;
    let token = std::env::var("DISCORD_TOKEN")
        .context("Defina a credencial local DISCORD_TOKEN; não a passe como argumento")?;
    let http = s::Http::new(&token);
    let bot = discord_server_bot::services::components_v2::private_admin_identity(&http).await?;
    let store = Store::open(
        std::env::var("DATA_DIR")
            .unwrap_or_else(|_| "data".into())
            .into(),
    )
    .context("Pare a instância do bot antes de reconciliar; lock exclusivo obrigatório")?;
    let _guard = store.gate.lock().await;
    let mut state = store.read(guild)?;
    println!("Guilda: {guild}\n{}", journal::describe(&state));
    if state.pending.is_none() && state.pending_operation.is_none() {
        return Ok(());
    }
    let channel = state
        .pending_operation
        .as_ref()
        .map(|p| p.channel)
        .or_else(|| {
            state
                .pending
                .as_deref()
                .and_then(|p| p.rsplit_once(':'))
                .and_then(|(key, _)| state.publications.get(key))
                .map(|p| p.channel)
        });
    if let Some(channel) = channel {
        // Verify real channel ownership, never rely solely on a local numeric ID.
        let real = channel
            .to_channel(&http)
            .await?
            .guild()
            .context("Canal não pertence a guilda")?;
        ensure!(
            real.guild_id == cfg.guild_id,
            "Canal pertence a outra guilda; nada alterado"
        );
    }
    let apply = args.iter().any(|a| a == "--apply");
    if value("--reject-code").is_some() || value("--reject-http").is_some() {
        let evidence = journal::Rejection {
            http_status: value("--reject-http")
                .context("Informe --reject-http")?
                .parse()?,
            discord_code: value("--reject-code")
                .context("Informe --reject-code")?
                .parse()?,
        };
        let confirmation = value("--confirm-pending")
            .context("Confirme exatamente o identificador completo de pending")?;
        ensure!(
            state.pending.as_deref() == Some(confirmation),
            "Confirmação divergente"
        );
        if !apply {
            println!(
                "Sem gravação. Confira a rejeição nos logs; --apply registra a evidência e arquiva o pending com backup, preservando publicações."
            );
            return Ok(());
        }
        journal::attest_rejection(
            &store,
            &mut state,
            guild,
            bot,
            &http,
            confirmation,
            evidence,
        )
        .await?;
        println!(
            "Rejeição atestada pelo operador; backup e histórico preservados. Nenhuma mensagem Discord modificada."
        );
    } else if let Some(evidence) = if let Some(id) = value("--message-id") {
        Some(
            journal::inspect_message(&state, guild, bot, &http, s::MessageId::new(id.parse()?))
                .await?,
        )
    } else {
        journal::inspect(&state, guild, bot, &http).await?
    } {
        if apply {
            journal::conclude(&store, &mut state, guild, evidence)?;
            println!("Reconciliação conclusiva salva com backup. Nenhuma escrita no Discord.");
        } else {
            println!(
                "Evidência conclusiva encontrada. Revise e execute com --apply para salvar a reconciliação."
            );
        }
    } else {
        println!(
            "Reconciliação inconclusiva. Pending preservado. Confira a requisição e os logs; uma busca vazia não prova que o envio falhou."
        );
    }
    Ok(())
}
