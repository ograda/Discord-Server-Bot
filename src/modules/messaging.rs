use crate::services::announcements::media::{self, ImageChange};
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
    #[description = "Imagem opcional enviada como anexo (PNG, JPEG, WebP ou GIF)"] imagem: Option<
        s::Attachment,
    >,
    #[description = "URL externa alternativa ao upload"] url_imagem: Option<String>,
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
    let target = publishing::authorize_channel(ctx, "announcements", channel, true).await?;
    if imagem.is_some() {
        publishing::authorize_attachment(ctx, &target).await?;
    }
    let base = storage::template(
        &ctx.data().templates,
        cfg,
        "announcements",
        &Default::default(),
    )?;
    let mut doc = announcements::with_message(&base, &mensagem)?;
    let change = media::input(
        None,
        imagem.as_ref(),
        url_imagem.as_deref(),
        media::upload_limit(ctx),
    )
    .await?;
    change.apply(&mut doc)?;
    let doc = announcements::build(ctx.http(), cfg.guild_id, &doc).await?;
    let mut send = s::CreateMessage::new()
        .embeds(doc.embeds("announcements")?)
        .allowed_mentions(s::CreateAllowedMentions::new());
    if let ImageChange::Upload(upload) = change {
        send = send.add_file(upload.attachment());
    }
    let message = channel.send_message(ctx.http(), send).await?;
    tracing::info!(guild = %cfg.guild_id, module = "announce", action = "send", stage = "complete", channel = %channel, message = %message.id, "Anúncio rápido enviado como embed");
    ctx.say(format!("Anúncio enviado em <#{channel}>.")).await?;
    Ok(())
}
