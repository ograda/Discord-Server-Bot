use crate::core::{Context, Error, permissions};
use anyhow::{Context as _, ensure};
use poise::serenity_prelude as s;

/// Gerencia cargos de membros.
#[poise::command(
    slash_command,
    guild_only,
    ephemeral,
    subcommand_required,
    subcommands("add", "remove"),
    default_member_permissions = "MANAGE_ROLES"
)]
pub async fn role(_: Context<'_>) -> Result<(), Error> {
    Ok(())
}

/// Adiciona um cargo a um membro.
#[poise::command(slash_command, guild_only, ephemeral)]
pub async fn add(
    ctx: Context<'_>,
    #[description = "Membro"] member: s::User,
    #[description = "Cargo"] role: s::Role,
) -> Result<(), Error> {
    change(ctx, member, role, true).await
}
/// Remove um cargo de um membro.
#[poise::command(slash_command, guild_only, ephemeral)]
pub async fn remove(
    ctx: Context<'_>,
    #[description = "Membro"] member: s::User,
    #[description = "Cargo"] role: s::Role,
) -> Result<(), Error> {
    change(ctx, member, role, false).await
}
async fn change(ctx: Context<'_>, user: s::User, role: s::Role, add: bool) -> Result<(), Error> {
    ctx.defer_ephemeral().await?;
    let cfg = permissions::guild_config(ctx)?;
    ensure!(
        cfg.modules
            .unwrap_or(ctx.data().config.global.modules)
            .roles,
        "Cargos desabilitados"
    );
    let auth = permissions::administrative(ctx, s::Permissions::MANAGE_ROLES).await?;
    permissions::ensure_bot_permissions(&auth, s::Permissions::MANAGE_ROLES)?;
    let role = auth
        .guild
        .roles
        .get(&role.id)
        .context("Cargo não pertence ao servidor")?;
    ensure!(
        !role.managed && role.id.get() != cfg.guild_id.get(),
        "Cargo gerenciado ou @everyone não pode ser alterado"
    );
    ensure!(
        permissions::higher(&auth.guild, &auth.bot, role),
        "Cargo deve estar abaixo do maior cargo do bot"
    );
    ensure!(
        auth.actor.user.id == auth.guild.owner_id
            || permissions::higher(&auth.guild, &auth.actor, role),
        "Cargo deve estar abaixo do seu maior cargo"
    );
    let member = auth.guild.id.member(ctx.http(), user.id).await?;
    ensure!(
        user.id != auth.guild.owner_id && user.id != auth.bot.user.id,
        "Não é permitido alterar cargos do dono ou do próprio bot"
    );
    for target_role in member
        .roles
        .iter()
        .filter_map(|id| auth.guild.roles.get(id))
    {
        ensure!(
            permissions::higher(&auth.guild, &auth.bot, target_role),
            "Membro está acima ou no mesmo nível do bot"
        );
        ensure!(
            auth.actor.user.id == auth.guild.owner_id
                || permissions::higher(&auth.guild, &auth.actor, target_role),
            "Membro está acima ou no mesmo nível que você"
        );
    }
    if add {
        member.add_role(ctx.http(), role.id).await?;
    } else {
        member.remove_role(ctx.http(), role.id).await?;
    }
    tracing::info!(guild = %cfg.guild_id, actor = %ctx.author().id, target = %user.id, role = %role.id, add, "Cargo alterado");
    ctx.say(if add {
        "Cargo adicionado."
    } else {
        "Cargo removido."
    })
    .await?;
    Ok(())
}
