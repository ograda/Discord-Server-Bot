use super::{Context, Error};
use crate::config::GuildConfig;
use anyhow::{Context as _, ensure};
use poise::serenity_prelude as s;

pub fn guild_config(ctx: Context<'_>) -> Result<&GuildConfig, Error> {
    let id = ctx
        .guild_id()
        .context("Comando disponível somente em servidores")?;
    ctx.data()
        .config
        .guilds
        .get(&id)
        .context("Servidor não autorizado")
}

pub fn authorize(
    has_permission: bool,
    owner: bool,
    restricted: bool,
    configured_role: bool,
) -> bool {
    owner || (has_permission && (!restricted || configured_role))
}

pub struct Authority {
    pub guild: s::PartialGuild,
    pub actor: s::Member,
    pub bot: s::Member,
}

pub async fn administrative(
    ctx: Context<'_>,
    required: s::Permissions,
) -> Result<Authority, Error> {
    let cfg = guild_config(ctx)?;
    ensure!(
        cfg.modules
            .unwrap_or(ctx.data().config.global.modules)
            .administration,
        "Administração desabilitada"
    );
    let guild = cfg.guild_id.to_partial_guild(ctx.http()).await?;
    let actor = guild.id.member(ctx.http(), ctx.author().id).await?;
    let bot_id = ctx.serenity_context().cache.current_user().id;
    let bot = guild.id.member(ctx.http(), bot_id).await?;
    let permissions = guild.member_permissions(&actor);
    ensure!(
        authorize(
            permissions.contains(required),
            actor.user.id == guild.owner_id,
            !cfg.roles.moderators.is_empty(),
            actor.roles.iter().any(|r| cfg.roles.moderators.contains(r))
        ),
        "Você não possui a permissão ou cargo administrativo exigido"
    );
    ensure!(
        guild.member_permissions(&bot).contains(required),
        "O bot não possui a permissão necessária"
    );
    Ok(Authority { guild, actor, bot })
}

pub fn higher(guild: &s::PartialGuild, member: &s::Member, role: &s::Role) -> bool {
    member
        .roles
        .iter()
        .filter_map(|id| guild.roles.get(id))
        .any(|r| r > role)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn authorization_matrix() {
        assert!(authorize(false, true, true, false));
        assert!(authorize(true, false, false, false));
        assert!(authorize(true, false, true, true));
        assert!(!authorize(true, false, true, false));
        assert!(!authorize(false, false, true, true));
        assert!(!authorize(false, false, false, false));
    }
}
