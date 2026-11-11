use crate::{
    core::{Context, Error, permissions},
    services::profile::{self as service, Patch},
};
use anyhow::{Result, ensure};
use poise::serenity_prelude as s;

async fn authorize(ctx: Context<'_>) -> Result<permissions::Authority> {
    let cfg = permissions::guild_config(ctx)?;
    ensure!(
        cfg.modules
            .unwrap_or(ctx.data().config.global.modules)
            .profile,
        "Módulo profile desabilitado"
    );
    permissions::administrative(ctx, s::Permissions::MANAGE_GUILD).await
}
/// Perfil do bot somente nesta guilda.
#[poise::command(
    slash_command,
    guild_only,
    ephemeral,
    subcommands("show", "nickname", "avatar", "bio", "apply"),
    default_member_permissions = "MANAGE_GUILD"
)]
pub async fn profile(_: Context<'_>) -> Result<(), Error> {
    Ok(())
}

/// Mostra o perfil atual do bot nesta guilda.
#[poise::command(slash_command, guild_only, ephemeral)]
async fn show(ctx: Context<'_>) -> Result<(), Error> {
    ctx.defer_ephemeral().await?;
    let auth = authorize(ctx).await?;
    let current = ctx
        .data()
        .profile_client
        .current(ctx.http().token(), auth.guild.id, auth.bot.user.id)
        .await?;
    let state = {
        let _guard = ctx.data().store.gate.lock().await;
        ctx.data().store.read(auth.guild.id)?
    };
    let bio = current.bio.clone().unwrap_or_else(|| {
        format!(
            "API não retornou a biografia. Última aplicada por este bot: {}",
            state
                .profile
                .applied_bio
                .as_deref()
                .unwrap_or("não disponível")
        )
    });
    let mut embed = s::CreateEmbed::new()
        .title("Perfil do bot nesta guilda")
        .field(
            "Servidor",
            format!("{} ({})", auth.guild.name, auth.guild.id),
            false,
        )
        .field(
            "Apelido",
            current
                .nick
                .as_deref()
                .unwrap_or("sem apelido; usa o nome global"),
            false,
        )
        .field(
            "Biografia",
            if bio.is_empty() { "(vazia)" } else { &bio },
            false,
        );
    let avatar = if let Some(hash) = current.avatar {
        format!(
            "https://cdn.discordapp.com/guilds/{}/users/{}/avatars/{}.{}",
            auth.guild.id,
            auth.bot.user.id,
            hash,
            if hash.starts_with("a_") { "gif" } else { "png" }
        )
    } else {
        auth.bot.user.face()
    };
    embed = embed.thumbnail(avatar);
    ctx.send(poise::CreateReply::default().embed(embed).ephemeral(true))
        .await?;
    Ok(())
}
/// Altera o apelido do bot apenas nesta guilda.
#[poise::command(slash_command, guild_only, ephemeral)]
async fn nickname(
    ctx: Context<'_>,
    #[description = "Novo apelido (1–32 caracteres)"] value: String,
) -> Result<(), Error> {
    ctx.defer_ephemeral().await?;
    service::validate_nickname(&value)?;
    update(
        ctx,
        Some(Patch {
            nick: Some(value),
            ..Default::default()
        }),
    )
    .await
}
/// Altera a biografia do bot apenas nesta guilda.
#[poise::command(slash_command, guild_only, ephemeral)]
async fn bio(
    ctx: Context<'_>,
    #[description = "Biografia (até 190 caracteres)"] value: String,
) -> Result<(), Error> {
    ctx.defer_ephemeral().await?;
    service::validate_bio(&value)?;
    update(
        ctx,
        Some(Patch {
            bio: Some(value),
            ..Default::default()
        }),
    )
    .await
}
/// Altera o avatar usando um anexo PNG, JPEG ou GIF.
#[poise::command(slash_command, guild_only, ephemeral)]
async fn avatar(
    ctx: Context<'_>,
    #[description = "Imagem de até 4 MiB"] image: s::Attachment,
) -> Result<(), Error> {
    ctx.defer_ephemeral().await?;
    authorize(ctx).await?;
    let data = ctx.data().profile_client.attachment(&image).await?;
    update(
        ctx,
        Some(Patch {
            avatar: Some(data),
            ..Default::default()
        }),
    )
    .await
}
/// Aplica personalizações persistidas e configurações TOML, somente se necessário.
#[poise::command(slash_command, guild_only, ephemeral)]
async fn apply(ctx: Context<'_>) -> Result<(), Error> {
    ctx.defer_ephemeral().await?;
    update(ctx, None).await
}
async fn update(ctx: Context<'_>, requested: Option<Patch>) -> Result<(), Error> {
    let auth = authorize(ctx).await?;
    let cfg = permissions::guild_config(ctx)?;
    let _guard = ctx.data().store.gate.lock().await;
    let mut state = ctx.data().store.read(auth.guild.id)?;
    let current = ctx
        .data()
        .profile_client
        .current(ctx.http().token(), auth.guild.id, auth.bot.user.id)
        .await?;
    let desired = requested
        .clone()
        .unwrap_or_else(|| service::effective(&cfg.profile, &state.profile));
    let has_requested = requested.is_some();
    let patch = service::changes(&desired, &current, &state.profile);
    patch.validate()?;
    if patch.nick.is_some() {
        permissions::ensure_bot_permissions(&auth, s::Permissions::CHANGE_NICKNAME)?;
    }
    let changed = !patch.is_empty();
    if changed {
        let result = ctx
            .data()
            .profile_client
            .update(ctx.http().token(), auth.guild.id, &patch)
            .await?;
        if let Some(data) = &patch.avatar {
            state.profile.applied_avatar_digest = Some(service::digest(data));
            state.profile.applied_avatar_hash = result.avatar;
        }
        if let Some(bio) = &patch.bio {
            state.profile.applied_bio = Some(bio.clone());
        }
    }
    if let Some(request) = requested {
        if request.nick.is_some() {
            state.profile.nickname = request.nick;
        }
        if request.bio.is_some() {
            state.profile.bio = request.bio;
        }
        if request.avatar.is_some() {
            state.profile.avatar = request.avatar;
        }
    }
    if changed || has_requested {
        ctx.data().store.write(auth.guild.id, &state).map_err(|_| anyhow::anyhow!("Perfil pode ter sido aplicado no Discord, mas o estado não foi salvo. Confira /profile show e o volume de dados antes de repetir."))?;
    }
    tracing::info!(guild = %auth.guild.id, actor = %ctx.author().id, changed, nickname = patch.nick.is_some(), bio = patch.bio.is_some(), avatar = patch.avatar.is_some(), "Operação de perfil concluída");
    ctx.say(if changed {
        "Perfil desta guilda atualizado e personalizações salvas."
    } else {
        "Não existem modificações pendentes no perfil desta guilda."
    })
    .await?;
    Ok(())
}
