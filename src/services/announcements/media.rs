//! Announcement image drafts are memory-only; published attachment metadata lives in guild JSON.
use super::super::{
    content::{Document, valid_url},
    storage::State,
};
use anyhow::{Context as _, Result, ensure};
use poise::serenity_prelude as s;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::io::Cursor;

pub const MAX_UPLOAD_BYTES: usize = 8 * 1024 * 1024;
const MAX_DECODE_BYTES: u64 = 128 * 1024 * 1024;

#[derive(Clone)]
pub struct Upload {
    filename: String,
    bytes: Vec<u8>,
}
impl Upload {
    pub fn attachment(&self) -> s::CreateAttachment {
        s::CreateAttachment::bytes(self.bytes.clone(), &self.filename)
    }
    pub fn image_url(&self) -> String {
        format!("attachment://{}", self.filename)
    }
}
#[derive(Clone, Default)]
pub enum ImageChange {
    #[default]
    Keep,
    Remove,
    External(String),
    Upload(Upload),
}
#[derive(Clone, Copy, poise::ChoiceParameter)]
pub enum ImageAction {
    #[name = "manter"]
    Keep,
    #[name = "substituir"]
    Replace,
    #[name = "remover"]
    Remove,
}
pub async fn input(
    action: Option<ImageAction>,
    attachment: Option<&s::Attachment>,
    url: Option<&str>,
    limit: usize,
) -> Result<ImageChange> {
    check_input(action, attachment.is_some(), url)?;
    let upload = if let Some(attachment) = attachment {
        Some(download(attachment, limit).await?)
    } else {
        None
    };
    prepared_input(action, upload, url)
}
pub fn check_input(action: Option<ImageAction>, upload: bool, url: Option<&str>) -> Result<()> {
    let url = url.filter(|url| !url.trim().is_empty());
    ensure!(
        !upload || url.is_none(),
        "Validação da imagem: escolha apenas upload OU URL alternativa"
    );
    let supplied = upload || url.is_some();
    match action {
        Some(ImageAction::Keep) => ensure!(
            !supplied,
            "Manter imagem atual não aceita upload ou URL; escolha substituir"
        ),
        Some(ImageAction::Remove) => {
            ensure!(
                !supplied,
                "Remover imagem não aceita upload ou URL simultâneos"
            );
        }
        Some(ImageAction::Replace) => ensure!(
            supplied,
            "Para substituir, informe imagem no slash command ou URL alternativa"
        ),
        None => (),
    }
    Ok(())
}
pub fn prepared_input(
    action: Option<ImageAction>,
    upload: Option<Upload>,
    url: Option<&str>,
) -> Result<ImageChange> {
    check_input(action, upload.is_some(), url)?;
    if matches!(action, Some(ImageAction::Remove)) {
        return Ok(ImageChange::Remove);
    }
    if let Some(upload) = upload {
        return Ok(ImageChange::Upload(upload));
    }
    if let Some(url) = url.filter(|url| !url.trim().is_empty()) {
        let change = ImageChange::External(url.into());
        change.apply(&mut Document::default())?;
        return Ok(change);
    }
    Ok(ImageChange::Keep)
}
impl ImageChange {
    pub fn apply(&self, doc: &mut Document) -> Result<()> {
        match self {
            Self::Keep => (),
            Self::Remove => doc.image.clear(),
            Self::External(url) => {
                valid_url(url).map_err(|_| anyhow::anyhow!("Validação da imagem: URL alternativa deve ser HTTP/HTTPS válida, sem credenciais"))?;
                doc.image = url.clone();
            }
            Self::Upload(upload) => doc.image = upload.image_url(),
        }
        Ok(())
    }
    pub fn files(&self) -> Vec<s::CreateAttachment> {
        match self {
            Self::Upload(upload) => vec![upload.attachment()],
            _ => vec![],
        }
    }
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PublishedImage {
    Attachment {
        id: s::AttachmentId,
        filename: String,
    },
    External {
        url: String,
    },
}

pub fn attachment_filename(url: &str) -> Option<&str> {
    let filename = url.strip_prefix("attachment://")?;
    (!filename.is_empty()
        && filename.len() <= 100
        && filename
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.'))
    .then_some(filename)
}
pub fn validate(bytes: &[u8], filename: &str, mime: Option<&str>, limit: usize) -> Result<Upload> {
    ensure!(
        !bytes.is_empty() && bytes.len() <= limit.min(MAX_UPLOAD_BYTES),
        "Validação da imagem: arquivo vazio ou acima do limite de upload (máximo 8 MiB)"
    );
    let format = image::guess_format(bytes)
        .map_err(|_| anyhow::anyhow!("Validação da imagem: conteúdo não reconhecido"))?;
    let (extension, expected_mime, extensions): (&str, &str, &[&str]) = match format {
        image::ImageFormat::Png => ("png", "image/png", &["png"]),
        image::ImageFormat::Jpeg => ("jpg", "image/jpeg", &["jpg", "jpeg"]),
        image::ImageFormat::WebP => ("webp", "image/webp", &["webp"]),
        image::ImageFormat::Gif => ("gif", "image/gif", &["gif"]),
        _ => anyhow::bail!("Validação da imagem: use PNG, JPEG, WebP ou GIF"),
    };
    let extension_input = filename
        .rsplit('.')
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    ensure!(
        extensions.contains(&extension_input.as_str()),
        "Validação da imagem: extensão incompatível com o conteúdo real"
    );
    if let Some(mime) = mime {
        ensure!(
            mime.split(';')
                .next()
                .unwrap_or_default()
                .trim()
                .eq_ignore_ascii_case(expected_mime),
            "Validação da imagem: MIME incompatível com o conteúdo real"
        );
    }
    let mut limits = image::Limits::default();
    limits.max_image_width = None;
    limits.max_image_height = None;
    limits.max_alloc = Some(MAX_DECODE_BYTES);
    let mut reader = image::ImageReader::with_format(Cursor::new(bytes), format);
    reader.limits(limits);
    let decoded = reader.decode().map_err(|_| anyhow::anyhow!("Processamento da imagem: arquivo malformado ou excede o orçamento de memória de 128 MiB"))?;
    ensure!(
        decoded.width() > 0 && decoded.height() > 0,
        "Processamento da imagem: dimensões inválidas"
    );
    drop(decoded);
    if format == image::ImageFormat::Gif {
        use image::{AnimationDecoder, ImageDecoder};
        let mut decoder = image::codecs::gif::GifDecoder::new(Cursor::new(bytes))
            .map_err(|_| anyhow::anyhow!("Processamento da imagem: GIF inválido"))?;
        let mut limits = image::Limits::default();
        limits.max_alloc = Some(MAX_DECODE_BYTES);
        decoder.set_limits(limits).map_err(|_| {
            anyhow::anyhow!("Processamento da imagem: GIF excede orçamento de memória")
        })?;
        let mut total = 0u64;
        for (index, frame) in decoder.into_frames().enumerate() {
            ensure!(
                index < 1000,
                "Processamento da imagem: animação excede o limite seguro de 1000 frames"
            );
            let frame = frame
                .map_err(|_| anyhow::anyhow!("Processamento da imagem: frame GIF inválido"))?;
            total = total.saturating_add(frame.buffer().as_raw().len() as u64);
            ensure!(
                total <= MAX_DECODE_BYTES,
                "Processamento da imagem: animação excede orçamento de processamento de 128 MiB"
            );
        }
    }
    let digest = format!("{:x}", Sha256::digest(bytes));
    Ok(Upload {
        filename: format!("announcement-{}.{}", &digest[..16], extension),
        bytes: bytes.to_vec(),
    })
}

pub fn upload_limit(ctx: crate::core::Context<'_>) -> usize {
    let remote = match ctx {
        poise::Context::Application(app) => app.interaction.attachment_size_limit as usize,
        _ => MAX_UPLOAD_BYTES,
    };
    if remote == 0 {
        MAX_UPLOAD_BYTES
    } else {
        remote.min(MAX_UPLOAD_BYTES)
    }
}
pub async fn download(attachment: &s::Attachment, limit: usize) -> Result<Upload> {
    let downloaded = super::super::attachments::download(
        attachment,
        limit.min(MAX_UPLOAD_BYTES),
        super::super::attachments::Purpose::AnnouncementImage,
    )
    .await?;
    let name = attachment.filename.clone();
    tokio::task::spawn_blocking(move || {
        validate(&downloaded.bytes, &name, downloaded.mime.as_deref(), limit)
    })
    .await
    .context("Processamento da imagem: tarefa interrompida")?
}

pub fn main_attachment<'a>(
    doc: &Document,
    message: &'a s::Message,
    saved: Option<&PublishedImage>,
) -> Option<&'a s::Attachment> {
    if let Some(PublishedImage::Attachment { id, .. }) = saved {
        return message.attachments.iter().find(|a| a.id == *id);
    }
    if let Some(filename) = attachment_filename(&doc.image) {
        return message.attachments.iter().find(|a| a.filename == filename);
    }
    message.attachments.iter().find(|a| {
        same_attachment_url(&a.url, &doc.image) || same_attachment_url(&a.proxy_url, &doc.image)
    })
}
fn same_attachment_url(current: &str, saved: &str) -> bool {
    if current == saved {
        return true;
    }
    let (Ok(current), Ok(saved)) = (url::Url::parse(current), url::Url::parse(saved)) else {
        return false;
    };
    matches!(
        current.host_str(),
        Some("cdn.discordapp.com" | "media.discordapp.net")
    ) && current.scheme() == "https"
        && saved.scheme() == "https"
        && current.host_str() == saved.host_str()
        && current.path() == saved.path()
}
pub fn edit_attachments(
    doc: &mut Document,
    change: &ImageChange,
    message: &s::Message,
    saved: Option<&PublishedImage>,
) -> Result<s::EditAttachments> {
    let old = main_attachment(doc, message, saved);
    let mut edits = s::EditAttachments::keep_all(message);
    match change {
        ImageChange::Keep => {
            if let Some(old) = old {
                doc.image = format!("attachment://{}", old.filename);
            } else {
                ensure!(
                    attachment_filename(&doc.image).is_none()
                        && !matches!(saved, Some(PublishedImage::Attachment { .. })),
                    "Anexo original não encontrado; escolha substituir ou remover a imagem"
                );
            }
        }
        _ => {
            if let Some(old) = old {
                edits = edits.remove(old.id);
            }
            change.apply(doc)?;
        }
    }
    if let ImageChange::Upload(upload) = change {
        edits = edits.add(upload.attachment());
    }
    Ok(edits)
}
pub fn published(doc: &Document, message: &s::Message) -> Result<Option<PublishedImage>> {
    if doc.image.is_empty() {
        return Ok(None);
    }
    if let Some(filename) = attachment_filename(&doc.image) {
        let attachment = message.attachments.iter().find(|a| a.filename == filename).context("Discord não confirmou o anexo publicado; confira o registro pending antes de repetir")?;
        return Ok(Some(PublishedImage::Attachment {
            id: attachment.id,
            filename: attachment.filename.clone(),
        }));
    }
    Ok(Some(PublishedImage::External {
        url: doc.image.clone(),
    }))
}
pub fn preview_image(
    doc: &mut Document,
    message: &s::Message,
    state: &State,
    key: &str,
) -> Result<()> {
    if let Some(attachment) = main_attachment(doc, message, state.announcement_images.get(key)) {
        doc.image = attachment.url.clone();
    } else {
        ensure!(
            attachment_filename(&doc.image).is_none(),
            "Anexo original não encontrado; substitua ou remova a imagem"
        );
    }
    Ok(())
}
