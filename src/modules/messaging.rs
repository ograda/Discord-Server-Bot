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
    let (_, revision) = crate::services::publishing::load(ctx, "announcements").await?;
    let doc = crate::services::content::Document {
        text: content,
        ..Default::default()
    };
    crate::services::publishing::preview(
        ctx,
        "announcements",
        format!("announcement-{}", ctx.id()),
        doc,
        revision,
        false,
    )
    .await
}
