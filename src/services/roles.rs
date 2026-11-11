//! Role policy, paginated REST planning and a sequential, rate-limited bulk executor.
use crate::{
    config::Roles,
    core::permissions::{self, Authority},
};
use anyhow::{Context, Result, ensure};
use poise::serenity_prelude as s;
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    future::Future,
    sync::{Arc, Mutex},
};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, poise::ChoiceParameter)]
#[serde(rename_all = "lowercase")]
pub enum Action {
    #[name = "add"]
    Add,
    #[name = "remove"]
    Remove,
}
impl Action {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Add => "add",
            Self::Remove => "remove",
        }
    }
    pub fn wants_role(self) -> bool {
        matches!(self, Self::Add)
    }
}

#[derive(Default)]
pub struct RoleLocks(Mutex<HashMap<s::GuildId, Arc<tokio::sync::Mutex<()>>>>);
impl RoleLocks {
    /// The same guild lock covers individual operations, bulk planning and confirmation.
    pub fn try_acquire(&self, guild: s::GuildId) -> Result<tokio::sync::OwnedMutexGuard<()>> {
        let lock = self
            .0
            .lock()
            .map_err(|_| anyhow::anyhow!("Controle de concorrência de cargos indisponível"))?
            .entry(guild)
            .or_default()
            .clone();
        lock.try_lock_owned().map_err(|_| anyhow::anyhow!("Já existe uma operação de cargos ou confirmação pendente nesta guilda. Aguarde a conclusão."))
    }
}

pub trait Api: Send + Sync {
    fn authority(&self) -> impl Future<Output = Result<Authority>> + Send;
    fn members(
        &self,
        after: Option<s::UserId>,
    ) -> impl Future<Output = Result<Vec<s::Member>>> + Send;
    fn member(&self, user: s::UserId) -> impl Future<Output = Result<s::Member>> + Send;
    fn change(
        &self,
        user: s::UserId,
        role: s::RoleId,
        action: Action,
        reason: &str,
    ) -> impl Future<Output = Result<()>> + Send;
}
pub struct DiscordApi<'a> {
    pub http: &'a s::Http,
    pub guild: s::GuildId,
    pub actor: s::UserId,
    pub bot: s::UserId,
}
impl Api for DiscordApi<'_> {
    async fn authority(&self) -> Result<Authority> {
        let guild = self.guild.to_partial_guild(self.http).await?;
        let actor = self.http.get_member(self.guild, self.actor).await?;
        let bot = self.http.get_member(self.guild, self.bot).await?;
        Ok(Authority { guild, actor, bot })
    }
    async fn members(&self, after: Option<s::UserId>) -> Result<Vec<s::Member>> {
        Ok(self.guild.members(self.http, Some(1000), after).await?)
    }
    async fn member(&self, user: s::UserId) -> Result<s::Member> {
        Ok(self.http.get_member(self.guild, user).await?)
    }
    async fn change(
        &self,
        user: s::UserId,
        role: s::RoleId,
        action: Action,
        reason: &str,
    ) -> Result<()> {
        // Serenity's standard HTTP client owns bucket/global 429 handling; never bypass it here.
        match action {
            Action::Add => {
                self.http
                    .add_member_role(self.guild, user, role, Some(reason))
                    .await?
            }
            Action::Remove => {
                self.http
                    .remove_member_role(self.guild, user, role, Some(reason))
                    .await?
            }
        }
        Ok(())
    }
}

pub fn validate_actor(auth: &Authority, policy: &Roles, bulk: bool) -> Result<()> {
    ensure!(
        auth.actor.guild_id == auth.guild.id && auth.bot.guild_id == auth.guild.id,
        "Membro administrativo não pertence à guilda atual"
    );
    let mut required = s::Permissions::MANAGE_ROLES;
    if bulk {
        required |= s::Permissions::MANAGE_GUILD;
    }
    ensure!(
        permissions::authorize(
            auth.guild
                .member_permissions(&auth.actor)
                .contains(required),
            auth.actor.user.id == auth.guild.owner_id,
            !policy.moderators.is_empty(),
            auth.actor
                .roles
                .iter()
                .any(|r| policy.moderators.contains(r))
        ),
        "Operação de cargos exige Gerenciar Cargos e, em massa, Gerenciar Servidor, além do cargo moderador configurado"
    );
    permissions::ensure_bot_permissions(auth, s::Permissions::MANAGE_ROLES)
}
pub fn validate_role<'a>(
    auth: &'a Authority,
    policy: &Roles,
    id: s::RoleId,
    bulk: bool,
) -> Result<&'a s::Role> {
    validate_actor(auth, policy, bulk)?;
    let role = auth
        .guild
        .roles
        .get(&id)
        .context("Cargo não pertence à guilda atual ou foi excluído")?;
    ensure!(
        role.guild_id == auth.guild.id,
        "Cargo não pertence à guilda atual"
    );
    ensure!(
        id.get() != auth.guild.id.get()
            && !role.managed
            && role.tags.bot_id.is_none()
            && role.tags.integration_id.is_none()
            && !role.tags.premium_subscriber
            && role.tags.subscription_listing_id.is_none()
            && !role.tags.available_for_purchase
            && !role.tags.guild_connections,
        "Cargo gerenciado, @everyone ou especial não atribuível manualmente"
    );
    ensure!(
        !policy.protected.contains(&id),
        "Cargo protegido na configuração desta guilda"
    );
    if bulk {
        ensure!(
            !policy.moderators.contains(&id)
                && !role.permissions.intersects(
                    s::Permissions::ADMINISTRATOR
                        | s::Permissions::MANAGE_GUILD
                        | s::Permissions::MANAGE_ROLES
                ),
            "Cargo administrativo/moderador protegido contra alterações em massa"
        );
    }
    ensure!(
        permissions::higher(&auth.guild, &auth.bot, role),
        "Cargo deve estar abaixo do maior cargo do bot"
    );
    ensure!(
        auth.actor.user.id == auth.guild.owner_id
            || permissions::higher(&auth.guild, &auth.actor, role),
        "Cargo deve estar abaixo do seu maior cargo"
    );
    Ok(role)
}

/// A reason is a fixed code, safe to persist and log without user-supplied content.
pub fn skip_reason(
    auth: &Authority,
    member: &s::Member,
    role: s::RoleId,
    action: Action,
    bulk: bool,
) -> Option<&'static str> {
    if member.guild_id != auth.guild.id {
        return Some("other_guild");
    }
    if bulk && member.user.bot {
        return Some("bot");
    }
    if member.user.id == auth.guild.owner_id
        || member.user.id == auth.bot.user.id
        || (bulk && member.user.id == auth.actor.user.id)
    {
        return Some("protected_member");
    }
    if bulk && member.pending {
        return Some("membership_screening");
    }
    if member.roles.contains(&role) == action.wants_role() {
        return Some("already_desired_state");
    }
    for id in &member.roles {
        let Some(target_role) = auth.guild.roles.get(id) else {
            return Some("unknown_member_role");
        };
        if !permissions::higher(&auth.guild, &auth.bot, target_role) {
            return Some("bot_hierarchy");
        }
        if auth.actor.user.id != auth.guild.owner_id
            && !permissions::higher(&auth.guild, &auth.actor, target_role)
        {
            return Some("actor_hierarchy");
        }
    }
    None
}

pub async fn change_one(
    api: &impl Api,
    policy: &Roles,
    guild: s::GuildId,
    role: s::RoleId,
    user: s::UserId,
    action: Action,
) -> Result<bool> {
    let auth = api.authority().await?;
    ensure!(
        auth.guild.id == guild,
        "Autoridade não pertence à guilda atual"
    );
    validate_role(&auth, policy, role, false)?;
    let member = api.member(user).await.map_err(|error| {
        anyhow::anyhow!(
            "Não foi possível obter o usuário nesta guilda: {}",
            safe_error(&error)
        )
    })?;
    ensure!(
        member.user.id == user && member.guild_id == guild,
        "Usuário não pertence à guilda atual"
    );
    if let Some(reason) = skip_reason(&auth, &member, role, action, false) {
        tracing::info!(%guild, command = "role", action = action.as_str(), stage = "validation", target = %user, %role, reason, "Operação individual ignorada");
        if reason == "already_desired_state" {
            return Ok(false);
        }
        anyhow::bail!(
            "Usuário protegido ou fora da hierarquia permitida ({reason}); nada alterado"
        );
    }
    api.change(
        user,
        role,
        action,
        &format!("role {} actor={}", action.as_str(), auth.actor.user.id),
    )
    .await?;
    tracing::info!(%guild, command = "role", action = action.as_str(), stage = "complete", target = %user, %role, "Cargo individual alterado");
    Ok(true)
}

pub struct Plan {
    pub guild: s::GuildId,
    pub actor: s::UserId,
    pub bot: s::UserId,
    pub role: s::RoleId,
    pub action: Action,
    pub targets: Vec<s::UserId>,
    pub scanned: u64,
    pub skipped: BTreeMap<String, u64>,
}
impl Plan {
    pub fn skipped_count(&self) -> u64 {
        self.skipped.values().sum()
    }
}

pub async fn plan(api: &impl Api, policy: &Roles, role: s::RoleId, action: Action) -> Result<Plan> {
    let auth = api.authority().await?;
    validate_role(&auth, policy, role, true)?;
    let mut plan = Plan {
        guild: auth.guild.id,
        actor: auth.actor.user.id,
        bot: auth.bot.user.id,
        role,
        action,
        targets: vec![],
        scanned: 0,
        skipped: BTreeMap::new(),
    };
    let mut after = None;
    let mut seen = HashSet::new();
    loop {
        let members = api.members(after).await.map_err(|e| anyhow::anyhow!("Falha ao listar todos os membros: {}. Confira GUILD_MEMBERS/Server Members Intent no TOML e no Developer Portal; nada executado.", safe_error(&e)))?;
        let count = members.len();
        ensure!(
            count <= 1000,
            "Resposta de paginação de membros inválida; nada executado"
        );
        let next = members.iter().map(|m| m.user.id).max();
        for member in members {
            ensure!(
                member.guild_id == plan.guild
                    && after.is_none_or(|cursor| member.user.id > cursor)
                    && seen.insert(member.user.id),
                "Paginação inconsistente ou membro de outra guilda; nada executado"
            );
            plan.scanned += 1;
            if let Some(reason) = skip_reason(&auth, &member, role, action, true) {
                *plan.skipped.entry(reason.into()).or_default() += 1;
                tracing::info!(guild = %plan.guild, module = "roles", command = "role-all", action = action.as_str(), stage = "planning", target = %member.user.id, role = %role, reason, "Membro ignorado");
            } else {
                plan.targets.push(member.user.id);
            }
        }
        if count < 1000 {
            break;
        }
        ensure!(
            next.is_some() && next > after,
            "Paginação sem avanço; nada executado"
        );
        after = next;
    }
    Ok(plan)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Report {
    pub operation: String,
    pub guild: s::GuildId,
    pub actor: s::UserId,
    pub role: s::RoleId,
    pub action: Action,
    pub scanned: u64,
    pub planned: u64,
    pub success: u64,
    pub failed: u64,
    pub skipped: BTreeMap<String, u64>,
    pub unprocessed: u64,
    pub status: String,
    pub stop_reason: Option<String>,
    pub started_at: i64,
    pub finished_at: Option<i64>,
}
impl Report {
    pub fn summary(&self) -> String {
        format!(
            "Operação `{}` • {}\nMembros listados: {} • elegíveis confirmados: {}\nSucessos: {} • falhas: {} • ignorados: {} • não processados: {}.{}",
            self.operation,
            self.status,
            self.scanned,
            self.planned,
            self.success,
            self.failed,
            self.skipped.values().sum::<u64>(),
            self.unprocessed,
            self.stop_reason
                .as_ref()
                .map(|r| format!("\nMotivo da interrupção: {r}"))
                .unwrap_or_default()
        )
    }
}
async fn save_report(store: &super::storage::Store, report: &Report) -> Result<()> {
    let _guard = store.gate.lock().await;
    let mut state = store.read(report.guild)?;
    state
        .role_operations
        .insert(report.operation.clone(), report.clone());
    state.revision += 1;
    store.write(report.guild, &state)
}
pub struct Outcome {
    pub report: Report,
    pub saved: bool,
}

/// Called only after author-bound confirmation, with the guild role lock still held.
/// Only confirmed target IDs are processed. No joins after confirmation expand the scope.
pub async fn execute(
    api: &impl Api,
    policy: &Roles,
    plan: Plan,
    operation: String,
    store: &super::storage::Store,
) -> Result<Outcome> {
    let mut report = Report {
        operation,
        guild: plan.guild,
        actor: plan.actor,
        role: plan.role,
        action: plan.action,
        scanned: plan.scanned,
        planned: plan.targets.len() as u64,
        success: 0,
        failed: 0,
        skipped: plan.skipped,
        unprocessed: plan.targets.len() as u64,
        status: "running".into(),
        stop_reason: None,
        started_at: s::Timestamp::now().unix_timestamp(),
        finished_at: None,
    };
    save_report(store, &report)
        .await
        .context("Não foi possível registrar a operação em massa; nenhum cargo foi alterado")?;
    let reason = format!(
        "role-all {} actor={} operation={}",
        plan.action.as_str(),
        plan.actor,
        report.operation
    );
    for (index, user) in plan.targets.iter().enumerate() {
        let fresh = async {
            let auth = api.authority().await?;
            ensure!(
                auth.guild.id == plan.guild
                    && auth.actor.user.id == plan.actor
                    && auth.bot.user.id == plan.bot,
                "Identidade administrativa alterada"
            );
            validate_role(&auth, policy, plan.role, true)?;
            Ok::<_, anyhow::Error>(auth)
        }
        .await;
        let auth = match fresh {
            Ok(auth) => auth,
            Err(error) => {
                report.stop_reason = Some(format!(
                    "Autorização/cargo mudou ou consulta falhou: {}",
                    safe_error(&error)
                ));
                break;
            }
        };
        let result = match api.member(*user).await {
            Ok(member) => {
                if member.user.id != *user {
                    report.stop_reason =
                        Some("Resposta retornou outro membro; operação interrompida".into());
                    break;
                }
                if let Some(skip) = skip_reason(&auth, &member, plan.role, plan.action, true) {
                    *report.skipped.entry(skip.into()).or_default() += 1;
                    tracing::info!(guild = %plan.guild, command = "role-all", action = plan.action.as_str(), stage = "execute", target = %user, role = %plan.role, reason = skip, "Membro ignorado após revalidação");
                    None
                } else {
                    Some(api.change(*user, plan.role, plan.action, &reason).await)
                }
            }
            Err(error) if http_status(&error) == Some(404) => {
                *report.skipped.entry("left_guild".into()).or_default() += 1;
                tracing::info!(guild = %plan.guild, command = "role-all", action = plan.action.as_str(), stage = "execute", target = %user, role = %plan.role, reason = "left_guild", "Membro ignorado");
                None
            }
            Err(error) => Some(Err(error)),
        };
        report.unprocessed = (plan.targets.len() - index - 1) as u64;
        if let Some(result) = result {
            match result {
                Ok(()) => {
                    report.success += 1;
                    tracing::info!(guild = %plan.guild, command = "role-all", action = plan.action.as_str(), stage = "execute", target = %user, role = %plan.role, "Cargo alterado");
                }
                Err(error) => {
                    report.failed += 1;
                    let status = http_status(&error);
                    tracing::warn!(guild = %plan.guild, command = "role-all", action = plan.action.as_str(), stage = "execute", target = %user, role = %plan.role, http_status = status, cause = safe_error(&error), "Falha individual na operação em massa");
                    if status.is_none_or(|s| s == 401 || s == 403 || s == 429 || s >= 500) {
                        report.stop_reason = Some(format!(
                            "Falha geral ou resultado incerto: {}",
                            safe_error(&error)
                        ));
                        break;
                    }
                }
            }
        }
        // Yield between members; Serenity awaits rate limit buckets and global cooldowns.
        tokio::task::yield_now().await;
        if (index + 1) % 25 == 0 && save_report(store, &report).await.is_err() {
            report.stop_reason = Some("Falha ao salvar progresso; execução interrompida".into());
            break;
        }
    }
    report.status = if report.stop_reason.is_some() {
        "stopped"
    } else {
        "completed"
    }
    .into();
    report.finished_at = Some(s::Timestamp::now().unix_timestamp());
    let saved = save_report(store, &report).await.is_ok();
    tracing::info!(guild = %report.guild, module = "roles", command = "role-all", action = report.action.as_str(), stage = "summary", operation = report.operation, success = report.success, failed = report.failed, skipped = report.skipped.values().sum::<u64>(), unprocessed = report.unprocessed, status = report.status, cause = report.stop_reason.as_deref(), saved, "Operação de cargos encerrada");
    Ok(Outcome { report, saved })
}
pub fn http_status(error: &anyhow::Error) -> Option<u16> {
    match error.downcast_ref::<s::Error>() {
        Some(s::Error::Http(s::HttpError::UnsuccessfulRequest(r))) => Some(r.status_code.as_u16()),
        _ => None,
    }
}
pub fn safe_error(error: &anyhow::Error) -> String {
    if let Some(status) = http_status(error) {
        format!("Discord HTTP {status}")
    } else if error.downcast_ref::<s::Error>().is_some() {
        "Falha de transporte ou resposta Discord inválida".into()
    } else {
        error.to_string()
    }
}
