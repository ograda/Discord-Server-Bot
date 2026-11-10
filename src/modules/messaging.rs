use crate::core::{Context, Error, permissions};
use anyhow::ensure;
use poise::serenity_prelude as s;

/// Publica um anúncio no canal permitido na configuração.
#[poise::command(
    slash_command,
    guild_only,
    ephemeral,
    default_member_permissions = "MANAGE_MESSAGES"
)]
pub async fn announce(
    ctx: Context<'_>,
    #[description = "Canal de anúncios autorizado"] channel: s::GuildChannel,
    #[description = "Texto do anúncio (até 2000 caracteres)"] content: String,
) -> Result<(), Error> {
    ctx.defer_ephemeral().await?;
    let cfg = permissions::guild_config(ctx)?;
    ensure!(
        cfg.modules
            .unwrap_or(ctx.data().config.global.modules)
            .messaging
            && cfg.messaging.enabled,
        "Mensagens desabilitadas"
    );
    ensure!(
        Some(channel.id) == cfg.channels.announcements && channel.guild_id == cfg.guild_id,
        "Canal não autorizado"
    );
    ensure!(
        !content.trim().is_empty() && content.chars().count() <= 2000,
        "Conteúdo deve ter entre 1 e 2000 caracteres"
    );
    let auth = permissions::administrative(ctx, s::Permissions::MANAGE_MESSAGES).await?;
    let channel = channel
        .id
        .to_channel(ctx.http())
        .await?
        .guild()
        .ok_or_else(|| anyhow::anyhow!("Canal inválido"))?;
    ensure!(
        matches!(channel.kind, s::ChannelType::Text | s::ChannelType::News),
        "Use um canal de texto ou anúncios"
    );
    let required = s::Permissions::VIEW_CHANNEL | s::Permissions::SEND_MESSAGES;
    ensure!(
        auth.guild
            .user_permissions_in(&channel, &auth.actor)
            .contains(required),
        "Você não pode publicar nesse canal"
    );
    ensure!(
        auth.guild
            .user_permissions_in(&channel, &auth.bot)
            .contains(required),
        "O bot não pode publicar nesse canal"
    );
    channel
        .id
        .send_message(
            ctx.http(),
            s::CreateMessage::new()
                .content(content)
                .allowed_mentions(s::CreateAllowedMentions::new()),
        )
        .await?;
    tracing::info!(guild = %cfg.guild_id, actor = %ctx.author().id, channel = %channel.id, action = "announce", "Anúncio publicado");
    ctx.say("Anúncio publicado.").await?;
    Ok(())
}
