use super::{Context, Error};
use crate::config::GuildConfig;
use anyhow::{Context as _, ensure};
use poise::serenity_prelude as s;

pub fn action(ctx: Context<'_>, root: &str, action: &str) -> Result<(), Error> {
    let guild = guild_config(ctx)?;
    guild.select_action(root, Some(action), ctx.data().config.global.modules)?;
    tracing::info!(guild = %guild.guild_id, module = root, command = ctx.command().qualified_name, action, stage = "authorization", "Ação selecionada");
    Ok(())
}

pub fn conditional_parameters(root: &str, action: &str, supplied: &[&str]) -> Result<(), Error> {
    let allowed: &[&str] = match (root, action) {
        ("links", _) => &["id"],
        ("announcement", "create") => &["mensagem", "imagem", "url_imagem", "imagem_acao"],
        ("announcement", "edit") => &[
            "identificador",
            "mensagem",
            "imagem",
            "url_imagem",
            "imagem_acao",
        ],
        ("announcement", "remove") => &["identificador"],
        ("profile", "nickname") => &["apelido"],
        ("profile", "bio") => &["biografia"],
        ("profile", "avatar") => &["imagem", "normalizar"],
        _ => &[],
    };
    ensure!(
        supplied.iter().all(|p| allowed.contains(p)),
        "Parâmetro incompatível com a ação {action}; remova os campos de outras ações"
    );
    Ok(())
}

pub fn guild_config(ctx: Context<'_>) -> Result<&GuildConfig, Error> {
    let id = ctx
        .guild_id()
        .context("Comando disponível somente em servidores")?;
    let guild = ctx
        .data()
        .config
        .guilds
        .get(&id)
        .context("Servidor não autorizado")?;
    ensure!(guild.enabled, "Servidor desabilitado");
    Ok(guild)
}

pub fn authorize(
    has_permission: bool,
    owner: bool,
    restricted: bool,
    configured_role: bool,
) -> bool {
    owner || (has_permission && (!restricted || configured_role))
}

#[derive(Clone)]
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
    Ok(Authority { guild, actor, bot })
}

pub fn ensure_permissions(actual: s::Permissions, required: s::Permissions) -> Result<(), Error> {
    ensure!(
        actual.contains(required),
        "Permissões operacionais ausentes no bot: {:?}",
        required & !actual
    );
    Ok(())
}

pub fn ensure_bot_permissions(auth: &Authority, required: s::Permissions) -> Result<(), Error> {
    ensure_permissions(auth.guild.member_permissions(&auth.bot), required)
}

pub fn ensure_bot_channel_permissions(
    auth: &Authority,
    channel: &s::GuildChannel,
    required: s::Permissions,
) -> Result<(), Error> {
    ensure!(
        channel.guild_id == auth.guild.id,
        "Canal pertence a outro servidor"
    );
    ensure_permissions(auth.guild.user_permissions_in(channel, &auth.bot), required)
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
    fn operational_permissions_are_independent() {
        let send = s::Permissions::VIEW_CHANNEL | s::Permissions::SEND_MESSAGES;
        assert!(ensure_permissions(send, send).is_ok());
        assert!(ensure_permissions(s::Permissions::VIEW_CHANNEL, send).is_err());
        assert!(ensure_permissions(send, send | s::Permissions::EMBED_LINKS).is_err());
        assert!(
            ensure_permissions(
                send | s::Permissions::EMBED_LINKS,
                send | s::Permissions::EMBED_LINKS
            )
            .is_ok()
        );
        assert!(ensure_permissions(send, s::Permissions::MANAGE_ROLES).is_err());
        assert!(
            ensure_permissions(s::Permissions::MANAGE_ROLES, s::Permissions::MANAGE_ROLES).is_ok()
        );
    }
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
