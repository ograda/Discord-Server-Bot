//! Single, unauthenticated CDN downloader for interaction Attachments. Never fetch external URLs.
use anyhow::{Result, ensure};
use poise::serenity_prelude as s;
use std::{future::Future, time::Duration};
use url::Url;

pub const MAX_BYTES: usize = 8 * 1024 * 1024;

#[derive(Clone, Copy)]
pub enum Purpose {
    ProfileAvatar,
    AnnouncementImage,
}
impl Purpose {
    fn module(self) -> &'static str {
        match self {
            Self::ProfileAvatar => "profile",
            Self::AnnouncementImage => "announcements",
        }
    }
}
pub struct Downloaded {
    pub bytes: Vec<u8>,
    pub mime: Option<String>,
}
impl std::fmt::Debug for Downloaded {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Downloaded")
            .field("bytes_len", &self.bytes.len())
            .finish_non_exhaustive()
    }
}

/// Query parameters are intentionally kept intact. Discord signs both normal and ephemeral URLs.
pub fn validate_url(value: &str) -> Result<Url> {
    let rejected = || {
        anyhow::anyhow!(
            "Download da imagem: somente anexos oficiais do Discord são aceitos (HTTPS, hostname exato e caminho de anexo válido)"
        )
    };
    if value.len() > 8192
        || value
            .chars()
            .any(|c| c.is_control() || c.is_whitespace() || c == '\\')
    {
        return Err(rejected());
    }
    let url = Url::parse(value).map_err(|_| rejected())?;
    let parts = url
        .path_segments()
        .map(|s| s.collect::<Vec<_>>())
        .unwrap_or_default();
    let valid_path = parts.len() == 4
        && matches!(parts[0], "attachments" | "ephemeral-attachments")
        && parts[1..3].iter().all(|id| {
            id.bytes().all(|b| b.is_ascii_digit()) && id.parse::<u64>().is_ok_and(|v| v > 0)
        })
        && !parts[3].is_empty()
        && !url.path().to_ascii_lowercase().contains("%2f")
        && !url.path().to_ascii_lowercase().contains("%5c");
    ensure!(
        url.scheme() == "https"
            && matches!(
                url.host_str(),
                Some("cdn.discordapp.com" | "media.discordapp.net")
            )
            && url.username().is_empty()
            && url.password().is_none()
            && url.port().is_none()
            && url.fragment().is_none()
            && valid_path,
        "{}",
        rejected()
    );
    Ok(url)
}

/// No raw URL, query, userinfo, Location header or binary data enters diagnostics.
pub fn diagnostic_location(value: &str) -> (String, String, &'static str) {
    match Url::parse(value) {
        Ok(url) => {
            let kind = if url.path().starts_with("/ephemeral-attachments/") {
                "ephemeral"
            } else if url.path().starts_with("/attachments/") {
                "conventional"
            } else {
                "invalid"
            };
            (
                url.host_str().unwrap_or("invalid").into(),
                url.path().into(),
                kind,
            )
        }
        Err(_) => ("invalid".into(), "invalid".into(), "invalid"),
    }
}
fn log_failure(
    attachment: &s::Attachment,
    purpose: Purpose,
    stage: &str,
    status: Option<u16>,
    cause: &str,
) {
    let (hostname, path, attachment_kind) = diagnostic_location(&attachment.url);
    tracing::warn!(
        module = purpose.module(),
        input_type = "slash_attachment",
        attachment_kind,
        stage,
        hostname,
        path,
        http_status = status,
        cause,
        "Falha no download compartilhado de anexo Discord"
    );
}
fn client() -> Result<reqwest::Client> {
    reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|_| anyhow::anyhow!("Download da imagem: falha ao preparar cliente HTTP"))
}
pub async fn download(
    attachment: &s::Attachment,
    limit: usize,
    purpose: Purpose,
) -> Result<Downloaded> {
    let http = client()?;
    download_with(attachment, limit, purpose, |url| async move {
        http.get(url)
            .send()
            .await
            .map_err(|_| anyhow::anyhow!("Download da imagem: falha de transporte ou timeout"))
    })
    .await
}
async fn download_with<F, Fut>(
    attachment: &s::Attachment,
    limit: usize,
    purpose: Purpose,
    send: F,
) -> Result<Downloaded>
where
    F: FnOnce(Url) -> Fut,
    Fut: Future<Output = Result<reqwest::Response>>,
{
    let limit = limit.min(MAX_BYTES);
    let url = validate_url(&attachment.url).inspect_err(|_| {
        log_failure(
            attachment,
            purpose,
            "url_validation",
            None,
            "URL fora da política de anexos",
        )
    })?;
    if attachment.size == 0 || attachment.size as usize > limit {
        log_failure(
            attachment,
            purpose,
            "size_validation",
            None,
            "Tamanho declarado inválido ou acima do limite",
        );
        anyhow::bail!("Download da imagem: anexo vazio ou acima do limite de {limit} bytes");
    }
    let response = send(url).await.inspect_err(|_| {
        log_failure(
            attachment,
            purpose,
            "request",
            None,
            "Falha de transporte ou timeout",
        )
    })?;
    let status = response.status().as_u16();
    let mut downloaded = read_response(response, limit).await.inspect_err(|_| {
        log_failure(
            attachment,
            purpose,
            "read_response",
            Some(status),
            "Resposta recusada, download interrompido ou acima do limite",
        )
    })?;
    if downloaded.bytes.len() != attachment.size as usize {
        log_failure(
            attachment,
            purpose,
            "size_validation",
            Some(status),
            "Tamanho recebido difere do declarado",
        );
        anyhow::bail!(
            "Download da imagem: tamanho recebido difere do anexo informado pelo Discord"
        );
    }
    downloaded.mime = attachment.content_type.clone().or(downloaded.mime);
    Ok(downloaded)
}
pub async fn read_response(mut response: reqwest::Response, limit: usize) -> Result<Downloaded> {
    let status = response.status().as_u16();
    ensure!(
        !response.status().is_redirection(),
        "Download da imagem: redirecionamento não autorizado (HTTP {status})"
    );
    ensure!(
        response.status().is_success(),
        "Download da imagem: HTTP {status}. O anexo pode ter expirado ou estar indisponível; envie o arquivo novamente"
    );
    let limit = limit.min(MAX_BYTES);
    ensure!(
        response.content_length().is_none_or(|n| n <= limit as u64),
        "Download da imagem: resposta acima do limite de {limit} bytes"
    );
    let mime = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|h| h.to_str().ok())
        .map(str::to_owned);
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| anyhow::anyhow!("Download da imagem: leitura interrompida"))?
    {
        ensure!(
            chunk.len() <= limit.saturating_sub(bytes.len()),
            "Download da imagem: resposta acima do limite de {limit} bytes"
        );
        bytes.extend_from_slice(&chunk);
    }
    Ok(Downloaded { bytes, mime })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Cursor, Read, Write};

    fn attachment(url: &str, size: usize) -> s::Attachment {
        serde_json::from_value(serde_json::json!({
            "id":"42", "filename":"image.png", "size":size, "content_type":"image/png",
            "url":url,"proxy_url":url, "ephemeral":url.contains("ephemeral-attachments")
        }))
        .unwrap()
    }
    fn response(status: u16, bytes: Vec<u8>) -> reqwest::Response {
        http::Response::builder()
            .status(status)
            .header("content-type", "image/png")
            .body(bytes)
            .unwrap()
            .into()
    }
    #[test]
    fn accepts_both_official_hosts_and_routes_preserving_the_signed_url() {
        for host in ["cdn.discordapp.com", "media.discordapp.net"] {
            for route in ["attachments", "ephemeral-attachments"] {
                let raw = format!(
                    "https://{host}/{route}/123/456/image.png?ex=abcdef&is=123456&hm=SIGNED%2BVALUE&width=1254"
                );
                let parsed = validate_url(&raw).unwrap();
                assert_eq!(parsed.as_str(), raw);
                assert_eq!(
                    parsed.query(),
                    Some("ex=abcdef&is=123456&hm=SIGNED%2BVALUE&width=1254")
                );
            }
        }
        assert!(
            validate_url("https://cdn.discordapp.com/attachments/1/2/image%20name.png").is_ok()
        );
    }
    #[test]
    fn rejects_external_malformed_spoofed_or_nonattachment_urls() {
        for raw in [
            "not a URL",
            "http://cdn.discordapp.com/attachments/1/2/image.png",
            "https://cdn.discordapp.com.evil.test/attachments/1/2/image.png",
            "https://evilcdn.discordapp.com/attachments/1/2/image.png",
            "https://cdn.discordapp.com@evil.test/attachments/1/2/image.png",
            "https://user:secret@cdn.discordapp.com/attachments/1/2/image.png",
            "https://cdn.discordapp.com:444/attachments/1/2/image.png",
            "https://cdn.discordapp.com/avatars/1/image.png",
            "https://cdn.discordapp.com/attachments/",
            "https://cdn.discordapp.com/attachments/1/image.png",
            "https://cdn.discordapp.com/attachments//2/image.png",
            "https://cdn.discordapp.com/attachments/a/2/image.png",
            "https://cdn.discordapp.com/attachments/1/2/",
            "https://cdn.discordapp.com/ephemeral-attachments-evil/1/2/image.png",
            "https://cdn.discordapp.com/attachments/1/2/a%2fb.png",
            "https://cdn.discordapp.com/attachments/1/2/a%5cb.png",
            "https://cdn.discordapp.com/attachments/1/2/image.png#private",
            "https://127.0.0.1/attachments/1/2/image.png",
            "https://cdn.discordapp.com./attachments/1/2/image.png",
        ] {
            assert!(validate_url(raw).is_err(), "{raw}");
        }
    }
    #[test]
    fn diagnostics_keep_only_hostname_path_and_kind_without_signature_or_userinfo() {
        let (host, path, kind) = diagnostic_location(
            "https://USER:PRIVATE@cdn.discordapp.com/ephemeral-attachments/123/456/image.png?ex=SECRET&is=SECRET&hm=SECRET",
        );
        assert_eq!(host, "cdn.discordapp.com");
        assert_eq!(path, "/ephemeral-attachments/123/456/image.png");
        assert_eq!(kind, "ephemeral");
        assert!(!format!("{host}{path}{kind}").contains("SECRET"));
        assert!(!format!("{host}{path}{kind}").contains("PRIVATE"));
    }
    #[test]
    fn serenity_resolves_the_original_temporary_attachment_with_signature_intact() {
        let raw = "https://cdn.discordapp.com/ephemeral-attachments/123/456/image.png?ex=aaa&is=bbb&hm=ccc";
        let payload: s::CommandData = serde_json::from_value(serde_json::json!({
            "id":"1","name":"announcement","type":1,
            "resolved":{"attachments":{"42":attachment(raw,100)}},
            "options":[{"name":"imagem","type":11,"value":"42"}]
        }))
        .unwrap();
        let options = payload.options();
        let s::ResolvedValue::Attachment(found) = options[0].value else {
            panic!("Attachment resolution required");
        };
        assert!(found.ephemeral);
        assert_eq!(found.url, raw);
        assert_eq!(validate_url(&found.url).unwrap().as_str(), raw);
    }
    #[tokio::test]
    async fn same_downloader_accepts_valid_png_for_profile_and_announcements() {
        let mut png = Cursor::new(Vec::new());
        image::DynamicImage::new_rgb8(1254, 1254)
            .write_to(&mut png, image::ImageFormat::Png)
            .unwrap();
        let png = png.into_inner();
        for purpose in [Purpose::ProfileAvatar, Purpose::AnnouncementImage] {
            for route in ["attachments", "ephemeral-attachments"] {
                let raw = format!(
                    "https://cdn.discordapp.com/{route}/123/456/image.png?ex=aaa&is=bbb&hm=ccc"
                );
                let input = attachment(&raw, png.len());
                let expected = raw.clone();
                let bytes = png.clone();
                let downloaded =
                    download_with(&input, 4 * 1024 * 1024, purpose, move |url| async move {
                        assert_eq!(url.as_str(), expected);
                        Ok(response(200, bytes))
                    })
                    .await
                    .unwrap();
                assert_eq!(downloaded.bytes, png);
                match purpose {
                    Purpose::ProfileAvatar => {
                        let avatar = crate::services::profile::prepare_avatar(
                            &downloaded.bytes,
                            downloaded.mime.as_deref(),
                            false,
                        )
                        .unwrap();
                        assert!(avatar.data.starts_with("data:image/png;base64,"));
                        assert_eq!((avatar.width, avatar.height), (1254, 1254));
                    }
                    Purpose::AnnouncementImage => {
                        let upload = crate::services::announcements::media::validate(
                            &downloaded.bytes,
                            "image.png",
                            downloaded.mime.as_deref(),
                            MAX_BYTES,
                        )
                        .unwrap();
                        assert_eq!(upload.attachment().data, png);
                        assert!(upload.image_url().starts_with("attachment://"));
                    }
                }
            }
        }
    }
    #[tokio::test]
    async fn invalid_urls_and_oversized_metadata_never_reach_http_transport() {
        let called = std::sync::atomic::AtomicBool::new(false);
        for (raw, size) in [
            ("https://evil.test/attachments/1/2/image.png", 1),
            ("https://cdn.discordapp.com/attachments/1/2/image.png", 1025),
        ] {
            let result = download_with(
                &attachment(raw, size),
                1024,
                Purpose::ProfileAvatar,
                |_| async {
                    called.store(true, std::sync::atomic::Ordering::SeqCst);
                    Ok(response(200, vec![]))
                },
            )
            .await;
            assert!(result.is_err());
            assert!(!called.load(std::sync::atomic::Ordering::SeqCst));
        }
    }
    #[tokio::test]
    async fn expired_or_unavailable_signature_has_clear_http_diagnostic_without_private_body() {
        for status in [403, 404] {
            let raw =
                "https://cdn.discordapp.com/ephemeral-attachments/1/2/image.png?ex=0&hm=PRIVATE";
            let error = download_with(
                &attachment(raw, 1),
                1024,
                Purpose::AnnouncementImage,
                |_| async move { Ok(response(status, b"PRIVATE RESPONSE BODY".to_vec())) },
            )
            .await
            .unwrap_err()
            .to_string();
            assert!(error.contains(&format!("HTTP {status}")));
            assert!(error.contains("expirado"));
            assert!(!error.contains("PRIVATE"));
        }
    }
    #[tokio::test]
    async fn downloader_never_follows_a_redirect_to_another_destination() {
        let target = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        target.set_nonblocking(true).unwrap();
        let source = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", source.local_addr().unwrap());
        let target_url = format!("http://{}/private?hm=PRIVATE", target.local_addr().unwrap());
        let server = std::thread::spawn(move || {
            let (mut socket, _) = source.accept().unwrap();
            let mut buffer = [0; 4096];
            let n = socket.read(&mut buffer).unwrap();
            assert!(
                !String::from_utf8_lossy(&buffer[..n])
                    .to_ascii_lowercase()
                    .contains("authorization:")
            );
            write!(socket,"HTTP/1.1 302 Found\r\nLocation: {target_url}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").unwrap();
        });
        // Transport policy is exercised locally; production download additionally enforces HTTPS CDN URLs.
        let response = client().unwrap().get(url).send().await.unwrap();
        assert_eq!(response.status().as_u16(), 302);
        assert!(
            read_response(response, 1024)
                .await
                .unwrap_err()
                .to_string()
                .contains("redirecionamento")
        );
        assert!(matches!(target.accept(),Err(e) if e.kind()==std::io::ErrorKind::WouldBlock));
        server.join().unwrap();
    }
    #[tokio::test]
    async fn byte_limits_mismatches_and_redacted_debug_are_preserved() {
        let raw = "https://cdn.discordapp.com/ephemeral-attachments/1/2/image.png?hm=PRIVATE";
        assert!(
            download_with(
                &attachment(raw, 1),
                1024,
                Purpose::AnnouncementImage,
                |_| async { Ok(response(200, vec![1, 2])) }
            )
            .await
            .is_err()
        );
        assert!(
            read_response(response(200, vec![0; 1025]), 1024)
                .await
                .is_err()
        );
        let data = Downloaded {
            bytes: b"PRIVATE BINARY".to_vec(),
            mime: None,
        };
        assert!(!format!("{data:?}").contains("PRIVATE"));
    }
}
