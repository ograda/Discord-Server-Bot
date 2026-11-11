use crate::{
    core::{Context, Error, permissions},
    services::{
        roles::{self as service, Action, DiscordApi},
        ui,
    },
};
use anyhow::ensure;
use poise::serenity_prelude as s;
use std::time::Duration;

/// Adiciona ou remove um cargo de um usuário da guilda, com seletores nativos.
#[poise::command(
    slash_command,
    guild_only,
    ephemeral,
    default_member_permissions = "MANAGE_ROLES"
)]
pub async fn role(
    ctx: Context<'_>,
    #[description = "Adicionar ou remover o cargo"] ação: Action,
    #[description = "Usuário da guilda atual"] usuário: s::User,
    #[description = "Cargo da guilda atual"] cargo: s::Role,
) -> Result<(), Error> {
    ctx.defer_ephemeral().await?;
    permissions::action(ctx, "role", ação.as_str())?;
    let cfg = permissions::guild_config(ctx)?;
    let _guard = ctx.data().role_locks.try_acquire(cfg.guild_id)?;
    let auth = permissions::administrative(ctx, s::Permissions::MANAGE_ROLES).await?;
    let api = DiscordApi {
        http: ctx.http(),
        guild: cfg.guild_id,
        actor: auth.actor.user.id,
        bot: auth.bot.user.id,
    };
    let changed =
        service::change_one(&api, &cfg.roles, cfg.guild_id, cargo.id, usuário.id, ação).await?;
    if !changed {
        ctx.say("O usuário já está no estado solicitado; nenhuma alteração necessária.")
            .await?;
        return Ok(());
    }
    ctx.say(if ação.wants_role() {
        "Cargo adicionado ao usuário desta guilda."
    } else {
        "Cargo removido do usuário desta guilda."
    })
    .await?;
    Ok(())
}

/// Altera um cargo dos humanos elegíveis, somente após contagem e confirmação.
#[poise::command(
    slash_command,
    guild_only,
    ephemeral,
    rename = "role-all",
    default_member_permissions = "MANAGE_ROLES | MANAGE_GUILD"
)]
pub async fn role_all(
    ctx: Context<'_>,
    #[description = "Adicionar ou remover o cargo dos humanos elegíveis"] ação: Action,
    #[description = "Cargo da guilda atual; cargos protegidos são bloqueados"] cargo: s::Role,
) -> Result<(), Error> {
    let reply_deadline = tokio::time::Instant::now() + Duration::from_secs(14 * 60);
    ctx.defer_ephemeral().await?;
    permissions::action(ctx, "role-all", ação.as_str())?;
    let cfg = permissions::guild_config(ctx)?;
    ensure!(
        ctx.data().config.global.intents.guild_members,
        "Operação em massa exige [intents] guild_members=true no default.toml e Server Members Intent habilitada no Developer Portal. /role individual continua disponível sem essa intent."
    );
    let _guard = ctx.data().role_locks.try_acquire(cfg.guild_id)?;
    let auth = permissions::administrative(
        ctx,
        s::Permissions::MANAGE_ROLES | s::Permissions::MANAGE_GUILD,
    )
    .await?;
    let api = DiscordApi {
        http: ctx.http(),
        guild: cfg.guild_id,
        actor: auth.actor.user.id,
        bot: auth.bot.user.id,
    };
    service::validate_role(&auth, &cfg.roles, cargo.id, true)?;
    ctx.say("Obtendo a lista completa de membros por paginação REST; nenhum cargo será alterado antes da confirmação.").await?;
    let plan = service::plan(&api, &cfg.roles, cargo.id, ação).await?;
    if plan.targets.is_empty() {
        ctx.say(format!("Nenhum humano elegível para alteração. Membros listados: {}; ignorados: {}. Nada executado.", plan.scanned, plan.skipped_count())).await?;
        return Ok(());
    }
    let prompt = format!(
        "Confirmar {} do cargo <@&{}> em **{} usuários humanos elegíveis** desta guilda?\nMembros listados: {} • ignorados: {}. Bots e membros protegidos não serão alterados. A elegibilidade será revalidada antes de cada alteração; novos membros não ampliarão o grupo confirmado.",
        ação.as_str(),
        cargo.id,
        plan.targets.len(),
        plan.scanned,
        plan.skipped_count()
    );
    if !ui::confirm(ctx, &prompt, "Confirmar operação em massa", None).await? {
        return Ok(());
    }
    permissions::action(ctx, "role-all", ação.as_str())?;
    permissions::administrative(
        ctx,
        s::Permissions::MANAGE_ROLES | s::Permissions::MANAGE_GUILD,
    )
    .await?;
    let operation = format!("role-all-{}", ctx.id());
    let progress = ctx.send(poise::CreateReply::default().content(format!("Operação `{operation}` iniciada. Processamento sequencial com rate limits; progresso e resumo ficam no estado JSON da guilda e nos logs.")).ephemeral(true)).await?;
    let execution = service::execute(&api, &cfg.roles, plan, operation.clone(), &ctx.data().store);
    tokio::pin!(execution);
    let mut interval = tokio::time::interval(Duration::from_secs(30));
    interval.tick().await;
    let outcome = loop {
        tokio::select! {
            result = &mut execution => break result?,
            _ = interval.tick() => {
                if tokio::time::Instant::now() >= reply_deadline { continue; }
                let report = { let _guard = ctx.data().store.gate.lock().await; ctx.data().store.read(cfg.guild_id).ok().and_then(|s| s.role_operations.get(&operation).cloned()) };
                if let Some(report) = report {
                    // Interaction tokens expire after 15 minutes; execution is independent of reply delivery.
                    let _ = tokio::time::timeout(Duration::from_secs(5), progress.edit(ctx, poise::CreateReply::default().content(report.summary()))).await;
                }
            }
        }
    };
    let mut summary = outcome.report.summary();
    if !outcome.saved {
        summary.push_str("\nO resumo final não foi salvo no JSON. Consulte os logs antes de repetir; algumas alterações podem ter sido aplicadas.");
    }
    let delivered = if tokio::time::Instant::now() < reply_deadline {
        tokio::time::timeout(
            Duration::from_secs(5),
            progress.edit(ctx, poise::CreateReply::default().content(summary)),
        )
        .await
        .is_ok_and(|result| result.is_ok())
    } else {
        false
    };
    if !delivered {
        tracing::warn!(guild = %cfg.guild_id, command = "role-all", action = ação.as_str(), stage = "summary_delivery", operation, "Resposta indisponível/expirada; resumo registrado nos logs e, se possível, no JSON da guilda");
    }
    Ok(())
}
