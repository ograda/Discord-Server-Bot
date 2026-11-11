use crate::{
    core::{Context, Error, permissions},
    services::{announcements, publishing, storage},
};
use poise::serenity_prelude as s;

/// Envia um embed rápido; canal ausente utiliza announcements da guilda.
#[poise::command(
    slash_command,
    guild_only,
    ephemeral,
    default_member_permissions = "MANAGE_MESSAGES"
)]
pub async fn announce(
    ctx: Context<'_>,
    #[description = "Mensagem do anúncio"] mensagem: String,
    #[description = "Canal de destino; padrão: announcements configurado"] canal: Option<
        s::GuildChannel,
    >,
) -> Result<(), Error> {
    ctx.defer_ephemeral().await?;
    let cfg = permissions::guild_config(ctx)?;
    let channel = match canal {
        Some(channel) => {
            anyhow::ensure!(
                channel.guild_id == cfg.guild_id,
                "Canal não pertence à guilda atual"
            );
            channel.id
        }
        None => cfg.channel("announcements")?,
    };
    publishing::authorize_channel(ctx, "announcements", channel, true).await?;
    let base = storage::template(
        &ctx.data().templates,
        cfg,
        "announcements",
        &Default::default(),
    )?;
    let doc = announcements::with_message(&base, &mensagem)?;
    let message = channel
        .send_message(
            ctx.http(),
            s::CreateMessage::new()
                .embeds(doc.embeds("announcements")?)
                .allowed_mentions(s::CreateAllowedMentions::new()),
        )
        .await?;
    tracing::info!(guild = %cfg.guild_id, module = "announce", action = "send", stage = "complete", channel = %channel, message = %message.id, "Anúncio rápido enviado como embed");
    ctx.say(format!("Anúncio enviado em <#{channel}>.")).await?;
    Ok(())
}
