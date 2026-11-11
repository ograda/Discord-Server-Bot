use anyhow::{Context, Result, ensure};
use base64::{Engine, engine::general_purpose::STANDARD};
use poise::serenity_prelude as s;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    io::{Cursor, Read},
    path::{Component, Path},
    time::Duration,
};

pub const MAX_AVATAR_BYTES: usize = 4 * 1024 * 1024;
pub const MAX_BIO: usize = 190;

#[derive(Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ProfileConfig {
    pub nickname: Option<String>,
    pub bio: Option<String>,
    pub avatar: Option<String>,
    #[serde(skip)]
    pub avatar_data: Option<String>,
}
impl std::fmt::Debug for ProfileConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProfileConfig")
            .field("nickname_configured", &self.nickname.is_some())
            .field("bio_configured", &self.bio.is_some())
            .field("avatar_configured", &self.avatar.is_some())
            .finish()
    }
}
impl ProfileConfig {
    pub fn validate(&self) -> Result<()> {
        if let Some(value) = &self.nickname {
            validate_nickname(value)?;
        }
        if let Some(value) = &self.bio {
            validate_bio(value)?;
        }
        Ok(())
    }
    pub fn load_avatar(&mut self, folder: &Path) -> Result<()> {
        self.validate()?;
        if let Some(path) = &self.avatar {
            self.avatar_data = Some(read_asset(folder, path)?);
        }
        Ok(())
    }
}
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ProfileState {
    pub nickname: Option<String>,
    pub bio: Option<String>,
    pub avatar: Option<String>,
    pub applied_avatar_digest: Option<String>,
    pub applied_avatar_hash: Option<String>,
    pub applied_bio: Option<String>,
}
#[derive(Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Patch {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nick: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bio: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avatar: Option<String>,
}
impl Patch {
    pub fn validate(&self) -> Result<()> {
        if let Some(value) = &self.nick {
            validate_nickname(value)?;
        }
        if let Some(value) = &self.bio {
            validate_bio(value)?;
        }
        if let Some(value) = &self.avatar {
            ensure!(
                value.len() <= MAX_AVATAR_BYTES.div_ceil(3) * 4 + 32,
                "Avatar persistido excede o limite"
            );
            let (header, encoded) = value
                .split_once(',')
                .context("Avatar persistido inválido")?;
            ensure!(
                matches!(
                    header,
                    "data:image/png;base64" | "data:image/jpeg;base64" | "data:image/gif;base64"
                ),
                "Formato de avatar persistido inválido"
            );
            let bytes = STANDARD
                .decode(encoded)
                .context("Avatar persistido inválido")?;
            ensure!(
                image_data(&bytes)? == *value,
                "Formato de avatar não corresponde à imagem"
            );
        }
        Ok(())
    }
    pub fn is_empty(&self) -> bool {
        self.nick.is_none() && self.bio.is_none() && self.avatar.is_none()
    }
}
#[derive(Clone, Default, Deserialize)]
pub struct MemberProfile {
    pub nick: Option<String>,
    pub avatar: Option<String>,
    pub bio: Option<String>,
}
pub fn validate_nickname(value: &str) -> Result<()> {
    ensure!(
        !value.trim().is_empty()
            && value.chars().count() <= 32
            && !value.chars().any(char::is_control),
        "Apelido deve ter 1–32 caracteres, sem controles"
    );
    Ok(())
}
pub fn validate_bio(value: &str) -> Result<()> {
    ensure!(
        value.chars().count() <= MAX_BIO && !value.contains('\0'),
        "Biografia deve ter no máximo 190 caracteres"
    );
    Ok(())
}
pub fn image_data(bytes: &[u8]) -> Result<String> {
    ensure!(
        !bytes.is_empty() && bytes.len() <= MAX_AVATAR_BYTES,
        "Avatar deve ter até 4 MiB"
    );
    let format = image::guess_format(bytes).context("Arquivo não é uma imagem reconhecida")?;
    let mime = match format {
        image::ImageFormat::Png => "image/png",
        image::ImageFormat::Jpeg => "image/jpeg",
        image::ImageFormat::Gif => "image/gif",
        _ => anyhow::bail!("Use PNG, JPEG ou GIF"),
    };
    let mut reader = image::ImageReader::with_format(Cursor::new(bytes), format);
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(4096);
    limits.max_image_height = Some(4096);
    limits.max_alloc = Some(64 * 1024 * 1024);
    reader.limits(limits);
    reader
        .decode()
        .context("Imagem inválida ou excede 4096×4096 pixels")?;
    Ok(format!("data:{mime};base64,{}", STANDARD.encode(bytes)))
}
pub fn read_asset(folder: &Path, path: &str) -> Result<String> {
    ensure!(
        !path.is_empty()
            && !path.contains('\\')
            && !path.contains(':')
            && Path::new(path)
                .components()
                .all(|c| matches!(c, Component::Normal(_))),
        "Avatar deve usar caminho relativo dentro da pasta da guilda, sem .. ou caminhos absolutos"
    );
    let root = folder
        .canonicalize()
        .context("Pasta da guilda não encontrada")?;
    let target = root
        .join(path)
        .canonicalize()
        .context("Arquivo de avatar não encontrado")?;
    ensure!(
        target.starts_with(&root) && target.is_file(),
        "Avatar aponta para fora da pasta da guilda"
    );
    let file = std::fs::File::open(target)?;
    ensure!(
        file.metadata()?.len() <= MAX_AVATAR_BYTES as u64,
        "Avatar excede 4 MiB"
    );
    let mut bytes = Vec::new();
    file.take(MAX_AVATAR_BYTES as u64 + 1)
        .read_to_end(&mut bytes)?;
    image_data(&bytes)
}
pub fn digest(data: &str) -> String {
    format!("{:x}", Sha256::digest(data.as_bytes()))
}
pub fn effective(config: &ProfileConfig, saved: &ProfileState) -> Patch {
    Patch {
        nick: saved.nickname.clone().or_else(|| config.nickname.clone()),
        bio: saved.bio.clone().or_else(|| config.bio.clone()),
        avatar: saved.avatar.clone().or_else(|| config.avatar_data.clone()),
    }
}
pub fn changes(desired: &Patch, current: &MemberProfile, saved: &ProfileState) -> Patch {
    let mut patch = desired.clone();
    if patch.nick.as_deref() == current.nick.as_deref() {
        patch.nick = None;
    }
    if let Some(bio) = &patch.bio
        && current.bio.as_ref().or(saved.applied_bio.as_ref()) == Some(bio)
    {
        patch.bio = None;
    }
    if let Some(avatar) = &patch.avatar
        && saved.applied_avatar_digest.as_ref() == Some(&digest(avatar))
        && saved.applied_avatar_hash == current.avatar
        && current.avatar.is_some()
    {
        patch.avatar = None;
    }
    patch
}
pub fn error_message(status: u16) -> &'static str {
    match status {
        400 => {
            "Discord recusou o perfil (400). Confira texto e imagem; nenhuma personalização foi salva."
        }
        403 => {
            "Discord negou a alteração (403). Confira acesso à guilda e CHANGE_NICKNAME para apelidos."
        }
        429 => {
            "Limite de alterações do Discord (429). Aguarde antes de tentar novamente; nenhum reenvio automático foi feito."
        }
        _ => "Falha na API de perfil. Confira o perfil atual antes de tentar novamente.",
    }
}
/// Isolated official REST extension: Serenity 0.12's member builder lacks bio/avatar.
/// Never instrument request bodies: they contain the avatar data URI.
pub struct Client {
    http: reqwest::Client,
    cooldowns: tokio::sync::Mutex<std::collections::HashMap<u64, std::time::Instant>>,
}
fn authorization(token: &str) -> Result<reqwest::header::HeaderValue> {
    let mut header = reqwest::header::HeaderValue::from_str(token)
        .map_err(|_| anyhow::anyhow!("Autorização do cliente Discord inválida"))?;
    header.set_sensitive(true);
    Ok(header)
}
impl Client {
    pub fn new() -> Result<Self> {
        Ok(Self {
            cooldowns: Default::default(),
            http: reqwest::Client::builder()
                .redirect(reqwest::redirect::Policy::none())
                .timeout(Duration::from_secs(30))
                .build()?,
        })
    }
    pub async fn current(
        &self,
        token: &str,
        guild: s::GuildId,
        bot: s::UserId,
    ) -> Result<MemberProfile> {
        self.check_cooldown(guild).await?;
        let response = self
            .http
            .get(format!(
                "https://discord.com/api/v10/guilds/{guild}/members/{bot}"
            ))
            .header(reqwest::header::AUTHORIZATION, authorization(token)?)
            .send()
            .await
            .map_err(|_| anyhow::anyhow!("Falha de transporte consultando o perfil"))?;
        self.response(guild, response).await
    }
    pub async fn update(
        &self,
        token: &str,
        guild: s::GuildId,
        patch: &Patch,
    ) -> Result<MemberProfile> {
        self.check_cooldown(guild).await?;
        let response = self.http.patch(format!("https://discord.com/api/v10/guilds/{guild}/members/@me")).header(reqwest::header::AUTHORIZATION, authorization(token)?).json(patch).send().await.map_err(|_| anyhow::anyhow!("Resultado da alteração de perfil incerto. Confira /profile show antes de tentar novamente."))?;
        self.response(guild, response).await
    }
    async fn check_cooldown(&self, guild: s::GuildId) -> Result<()> {
        let cooldowns = self.cooldowns.lock().await;
        for id in [0, guild.get()] {
            if let Some(until) = cooldowns.get(&id) {
                ensure!(
                    *until <= std::time::Instant::now(),
                    "Perfil em cooldown do Discord. Aguarde antes de tentar novamente."
                );
            }
        }
        Ok(())
    }
    async fn response(
        &self,
        guild: s::GuildId,
        response: reqwest::Response,
    ) -> Result<MemberProfile> {
        if response.status().as_u16() == 429 {
            #[derive(Deserialize)]
            struct RateLimit {
                retry_after: f64,
                #[serde(default)]
                global: bool,
            }
            let fallback = response
                .headers()
                .get("retry-after")
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.parse().ok())
                .unwrap_or(60.0);
            let body = response.json::<RateLimit>().await.unwrap_or(RateLimit {
                retry_after: fallback,
                global: false,
            });
            let delay = if body.retry_after.is_finite() {
                body.retry_after.clamp(1.0, 86400.0)
            } else {
                60.0
            };
            self.cooldowns.lock().await.insert(
                if body.global { 0 } else { guild.get() },
                std::time::Instant::now() + Duration::from_secs_f64(delay),
            );
            tracing::warn!(guild = %guild, retry_after = delay, "Discord limitou alterações de perfil");
            anyhow::bail!(error_message(429));
        }
        parse_response(response).await
    }
    pub async fn attachment(&self, attachment: &s::Attachment) -> Result<String> {
        ensure!(
            attachment.size as usize <= MAX_AVATAR_BYTES,
            "Anexo excede 4 MiB"
        );
        let url = url::Url::parse(&attachment.url)?;
        ensure!(
            url.scheme() == "https"
                && matches!(
                    url.host_str(),
                    Some("cdn.discordapp.com" | "media.discordapp.net")
                )
                && url.path().starts_with("/attachments/"),
            "Use um anexo enviado ao Discord"
        );
        let mut response = self
            .http
            .get(url)
            .send()
            .await
            .context("Não foi possível obter o anexo")?;
        ensure!(response.status().is_success(), "Anexo indisponível");
        if let Some(size) = response.content_length() {
            ensure!(size <= MAX_AVATAR_BYTES as u64, "Anexo excede 4 MiB");
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await? {
            ensure!(
                bytes.len() + chunk.len() <= MAX_AVATAR_BYTES,
                "Anexo excede 4 MiB"
            );
            bytes.extend_from_slice(&chunk);
        }
        image_data(&bytes)
    }
}
async fn parse_response(response: reqwest::Response) -> Result<MemberProfile> {
    let status = response.status().as_u16();
    if !(200..300).contains(&status) {
        tracing::warn!(status, "Falha na API de perfil por guilda");
        anyhow::bail!(error_message(status));
    }
    response.json().await.map_err(|_| {
        anyhow::anyhow!("Resposta de perfil inesperada do Discord; consulte /profile show")
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn mock(status: u16, body: &str) -> reqwest::Response {
        http::Response::builder()
            .status(status)
            .header("content-type", "application/json")
            .body(body.to_owned())
            .unwrap()
            .into()
    }
    #[tokio::test]
    async fn api_responses_are_typed_and_errors_do_not_leak_bodies() {
        let result = parse_response(mock(200, r#"{"nick":"Bot","avatar":"hash","bio":"Texto"}"#))
            .await
            .unwrap();
        assert_eq!(result.nick.as_deref(), Some("Bot"));
        for code in [400, 403, 429, 500] {
            let error = parse_response(mock(code, "SECRET_BASE64_TOKEN"))
                .await
                .err()
                .unwrap();
            assert!(!error.to_string().contains("SECRET"));
            assert_eq!(error.to_string(), error_message(code));
        }
    }
    #[tokio::test]
    async fn rate_limits_block_repeated_calls_without_network() {
        let client = Client::new().unwrap();
        let guild = s::GuildId::new(1);
        assert!(
            client
                .response(guild, mock(429, r#"{"retry_after":60,"global":false}"#))
                .await
                .is_err()
        );
        assert!(client.check_cooldown(guild).await.is_err());
        assert!(client.check_cooldown(s::GuildId::new(2)).await.is_ok());
        assert!(
            client
                .response(guild, mock(429, r#"{"retry_after":60,"global":true}"#))
                .await
                .is_err()
        );
        assert!(client.check_cooldown(s::GuildId::new(2)).await.is_err());
    }
}
