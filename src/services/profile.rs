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
#[derive(Clone, Default, Deserialize, PartialEq)]
pub struct Patch {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nick: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bio: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avatar: Option<String>,
    #[serde(skip)]
    pub clear_nick: bool,
    #[serde(skip)]
    pub clear_bio: bool,
    #[serde(skip)]
    pub clear_avatar: bool,
}
impl Serialize for Patch {
    fn serialize<S: serde::Serializer>(
        &self,
        serializer: S,
    ) -> std::result::Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;
        let mut map = serializer.serialize_map(None)?;
        for (key, value, clear) in [
            ("nick", &self.nick, self.clear_nick),
            ("bio", &self.bio, self.clear_bio),
            ("avatar", &self.avatar, self.clear_avatar),
        ] {
            if value.is_some() || clear {
                map.serialize_entry(key, value)?;
            }
        }
        map.end()
    }
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
        self.nick.is_none()
            && self.bio.is_none()
            && self.avatar.is_none()
            && !self.clear_nick
            && !self.clear_bio
            && !self.clear_avatar
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
    Ok(prepare_avatar(bytes, None, false)?.data)
}

pub struct PreparedAvatar {
    pub data: String,
    pub width: u32,
    pub height: u32,
    pub output_bytes: usize,
    pub mime: &'static str,
    pub normalized: bool,
}

/// Opt-in only: contain the whole image on a transparent square, without stretching.
pub fn prepare_avatar(
    bytes: &[u8],
    declared_mime: Option<&str>,
    normalize: bool,
) -> Result<PreparedAvatar> {
    ensure!(
        !bytes.is_empty() && bytes.len() <= MAX_AVATAR_BYTES,
        "Validação do avatar: imagem vazia ou excede 4 MiB"
    );
    let format = image::guess_format(bytes)
        .context("Validação do avatar: arquivo não é PNG, JPEG ou GIF reconhecido")?;
    let mime = match format {
        image::ImageFormat::Png => "image/png",
        image::ImageFormat::Jpeg => "image/jpeg",
        image::ImageFormat::Gif => "image/gif",
        _ => anyhow::bail!("Validação do avatar: use PNG, JPEG ou GIF"),
    };
    if let Some(declared) = declared_mime {
        let declared = declared
            .split(';')
            .next()
            .unwrap_or("")
            .trim()
            .to_ascii_lowercase();
        ensure!(
            declared == mime || declared == "application/octet-stream",
            "Validação do avatar: MIME declarado incompatível com os bytes da imagem"
        );
    }
    let mut reader = image::ImageReader::with_format(Cursor::new(bytes), format);
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(4096);
    limits.max_image_height = Some(4096);
    limits.max_alloc = Some(64 * 1024 * 1024);
    reader.limits(limits);
    let decoded = reader.decode().context(
        "Processamento do avatar: decodificação inválida ou imagem excede 4096×4096 pixels",
    )?;
    let (width, height) = (decoded.width(), decoded.height());
    let normalized =
        normalize && (width != 512 || height != 512 || format != image::ImageFormat::Png);
    let mut output = Cursor::new(Vec::new());
    let (bytes, mime) = if normalized {
        let resized = decoded
            .resize(512, 512, image::imageops::FilterType::Lanczos3)
            .to_rgba8();
        let mut square = image::RgbaImage::new(512, 512);
        image::imageops::overlay(
            &mut square,
            &resized,
            i64::from((512 - resized.width()) / 2),
            i64::from((512 - resized.height()) / 2),
        );
        image::DynamicImage::ImageRgba8(square)
            .write_to(&mut output, image::ImageFormat::Png)
            .context("Processamento do avatar: falha na conversão para PNG")?;
        (output.get_ref().as_slice(), "image/png")
    } else {
        (bytes, mime)
    };
    ensure!(
        bytes.len() <= MAX_AVATAR_BYTES,
        "Processamento do avatar: PNG resultante excede 4 MiB"
    );
    tracing::info!(
        width,
        height,
        output_bytes = bytes.len(),
        mime,
        normalized,
        "Imagem de avatar validada e codificada como Data URI"
    );
    Ok(PreparedAvatar {
        data: format!("data:{mime};base64,{}", STANDARD.encode(bytes)),
        width,
        height,
        output_bytes: bytes.len(),
        mime,
        normalized,
    })
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
        ..Default::default()
    }
}
pub fn revert(config: &ProfileConfig) -> Patch {
    Patch {
        nick: config.nickname.clone(),
        bio: config.bio.clone(),
        avatar: config.avatar_data.clone(),
        clear_nick: config.nickname.is_none(),
        clear_bio: config.bio.is_none(),
        clear_avatar: config.avatar_data.is_none(),
    }
}
pub fn clear_customizations(state: &mut ProfileState) {
    state.nickname = None;
    state.bio = None;
    state.avatar = None;
}
pub struct Operation {
    pub guild: s::GuildId,
    pub bot: s::UserId,
    pub desired: Patch,
    pub revert: bool,
}
/// Only persist a customization/revert after all required HTTP requests succeeded.
pub async fn apply(
    client: &Client,
    token: &str,
    store: &super::storage::Store,
    operation: Operation,
) -> Result<bool> {
    operation.desired.validate()?;
    let _guard = store.gate.lock().await;
    let mut state = store.read(operation.guild)?;
    let current = client
        .current(token, operation.guild, operation.bot)
        .await?;
    let patch = changes(&operation.desired, &current, &state.profile);
    let changed = !patch.is_empty();
    if changed {
        let result = client.update(token, operation.guild, &patch).await?;
        if let Some(data) = &patch.avatar {
            state.profile.applied_avatar_digest = Some(digest(data));
            state.profile.applied_avatar_hash = result.avatar;
        }
        if patch.clear_avatar {
            state.profile.applied_avatar_digest = None;
            state.profile.applied_avatar_hash = None;
        }
        if patch.bio.is_some() || patch.clear_bio {
            state.profile.applied_bio = operation.desired.bio.clone();
        }
    }
    if operation.revert {
        clear_customizations(&mut state.profile);
    } else {
        if operation.desired.nick.is_some() {
            state.profile.nickname = operation.desired.nick;
        }
        if operation.desired.bio.is_some() {
            state.profile.bio = operation.desired.bio;
        }
        if operation.desired.avatar.is_some() {
            state.profile.avatar = operation.desired.avatar;
        }
    }
    state.revision += 1;
    store.write(operation.guild, &state).map_err(|_| anyhow::anyhow!("Perfil pode ter sido aplicado no Discord, mas o estado não foi salvo. Confira o perfil do bot e o armazenamento antes de repetir."))?;
    Ok(changed)
}
pub fn load_defaults(root: &Path, guild: &crate::config::GuildConfig) -> Result<ProfileConfig> {
    let (defaults, specific) = super::catalog::paths(root, guild, "config")?;
    let mut base = crate::config::loader::optional(&defaults)?;
    let local = crate::config::loader::optional(&specific)?;
    let local_avatar = local.get("profile").and_then(|v| v.get("avatar")).is_some();
    crate::config::loader::merge(&mut base, local);
    let mut config: ProfileConfig = base
        .get("profile")
        .cloned()
        .unwrap_or_else(|| toml::Value::Table(Default::default()))
        .try_into()?;
    let folder = if local_avatar {
        specific.parent().unwrap()
    } else {
        defaults.parent().unwrap()
    };
    config.load_avatar(folder)?;
    Ok(config)
}
pub fn changes(desired: &Patch, current: &MemberProfile, saved: &ProfileState) -> Patch {
    let mut patch = desired.clone();
    if patch.clear_nick && current.nick.is_none() {
        patch.clear_nick = false;
    }
    if patch.clear_bio
        && current
            .bio
            .as_ref()
            .or(saved.applied_bio.as_ref())
            .is_none()
    {
        patch.clear_bio = false;
    }
    if patch.clear_avatar && current.avatar.is_none() {
        patch.clear_avatar = false;
    }
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
        401 => {
            "Discord recusou a autenticação (401). Confira o token do bot; nenhuma personalização foi salva."
        }
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
    api_root: String,
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
            api_root: "https://discord.com/api/v10".into(),
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
            .get(format!("{}/guilds/{guild}/members/{bot}", self.api_root))
            .header(reqwest::header::AUTHORIZATION, authorization(token)?)
            .send()
            .await
            .map_err(|_| anyhow::anyhow!("Falha de transporte consultando o perfil"))?;
        self.response(guild, response, "GET /guilds/{guild}/members/{bot}")
            .await
    }
    pub async fn update(
        &self,
        token: &str,
        guild: s::GuildId,
        patch: &Patch,
    ) -> Result<MemberProfile> {
        patch
            .validate()
            .context("Validação do payload de perfil falhou")?;
        self.check_cooldown(guild).await?;
        tracing::info!(%guild, method = "PATCH", route = "/guilds/{guild}/members/@me", avatar = patch.avatar.is_some(), nick = patch.nick.is_some(), bio = patch.bio.is_some(), payload_bytes = serde_json::to_vec(patch)?.len(), "Enviando perfil à API Discord");
        let response = self.http.patch(format!("{}/guilds/{guild}/members/@me", self.api_root)).header(reqwest::header::AUTHORIZATION, authorization(token)?).json(patch).send().await.map_err(|_| anyhow::anyhow!("Requisição HTTP de perfil: falha de transporte ou timeout; resultado incerto. Confira o perfil do bot nesta guilda antes de tentar novamente."))?;
        let result = self
            .response(guild, response, "PATCH /guilds/{guild}/members/@me")
            .await?;
        ensure!(
            patch.avatar.is_none() || result.avatar.as_ref().is_some_and(|hash| !hash.is_empty()),
            "Resposta HTTP de perfil: Discord não confirmou o avatar da guilda; nada persistido. Confira o perfil do bot nesta guilda antes de repetir."
        );
        ensure!(
            !patch.clear_avatar || result.avatar.is_none(),
            "Resposta HTTP não confirmou a remoção do avatar da guilda; nada persistido"
        );
        Ok(result)
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
        route: &'static str,
    ) -> Result<MemberProfile> {
        tracing::info!(%guild, module = "profile", stage = "profile_http", route, status = response.status().as_u16(), "Resposta HTTP de perfil recebida");
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
    pub async fn attachment(
        &self,
        attachment: &s::Attachment,
        normalize: bool,
    ) -> Result<PreparedAvatar> {
        let downloaded = super::attachments::download(
            attachment,
            MAX_AVATAR_BYTES,
            super::attachments::Purpose::ProfileAvatar,
        )
        .await?;
        tokio::task::spawn_blocking(move || {
            prepare_avatar(&downloaded.bytes, downloaded.mime.as_deref(), normalize)
        })
        .await
        .context("Processamento do avatar: tarefa interrompida")?
    }
}

async fn parse_response(response: reqwest::Response) -> Result<MemberProfile> {
    let status = response.status().as_u16();
    if !(200..300).contains(&status) {
        // Never echo Discord's free-form message/body: it may contain supplied data.
        let body = response
            .json::<serde_json::Value>()
            .await
            .unwrap_or_default();
        let code = body.get("code").and_then(|v| v.as_u64());
        let fields: Vec<_> = ["avatar", "nick", "bio"]
            .into_iter()
            .filter(|field| body.get("errors").and_then(|v| v.get(*field)).is_some())
            .collect();
        tracing::warn!(status, discord_code = code, rejected_fields = ?fields, "Falha na API de perfil por guilda");
        let details = format!(
            " Diagnóstico HTTP: status {status}, código Discord {}, campos rejeitados: {}.",
            code.map(|c| c.to_string())
                .unwrap_or_else(|| "indisponível".into()),
            if fields.is_empty() {
                "não informados".into()
            } else {
                fields.join(", ")
            }
        );
        anyhow::bail!("{}{details}", error_message(status));
    }
    response.json().await.map_err(|_| {
        anyhow::anyhow!(
            "Resposta de perfil inesperada do Discord; confira o perfil do bot nesta guilda"
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png(width: u32, height: u32) -> Vec<u8> {
        let mut output = Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
            width,
            height,
            image::Rgba([255, 0, 0, 255]),
        ))
        .write_to(&mut output, image::ImageFormat::Png)
        .unwrap();
        output.into_inner()
    }

    #[test]
    fn avatar_normalization_is_opt_in_and_preserves_proportions() {
        for size in [512, 1254] {
            let bytes = png(size, size);
            let prepared = prepare_avatar(&bytes, Some("image/png"), false).unwrap();
            assert!(!prepared.normalized);
            assert_eq!(
                STANDARD
                    .decode(prepared.data.split_once(',').unwrap().1)
                    .unwrap(),
                bytes
            );
            assert_eq!((prepared.width, prepared.height), (size, size));
        }
        let bytes = png(512, 512);
        assert!(!prepare_avatar(&bytes, None, true).unwrap().normalized);
        let prepared = prepare_avatar(&png(1254, 627), Some("image/png"), true).unwrap();
        assert!(prepared.normalized);
        let decoded = image::load_from_memory(
            &STANDARD
                .decode(prepared.data.split_once(',').unwrap().1)
                .unwrap(),
        )
        .unwrap()
        .to_rgba8();
        assert_eq!(decoded.dimensions(), (512, 512));
        assert_eq!(decoded.get_pixel(256, 0).0[3], 0); // Transparent letterboxing.
        assert_eq!(decoded.get_pixel(256, 128).0, [255, 0, 0, 255]);
        assert_eq!(decoded.get_pixel(256, 383).0, [255, 0, 0, 255]);
        assert_eq!(decoded.get_pixel(256, 511).0[3], 0);
    }

    #[test]
    fn avatar_validation_and_processing_have_distinct_diagnostics() {
        let bytes = png(512, 512);
        for mime in [
            None,
            Some("application/octet-stream"),
            Some("image/png; charset=binary"),
            Some("IMAGE/PNG"),
        ] {
            assert!(prepare_avatar(&bytes, mime, false).is_ok());
        }
        let err = prepare_avatar(&bytes, Some("image/jpeg"), false)
            .err()
            .unwrap();
        assert!(err.to_string().contains("Validação"));
        let err = prepare_avatar(b"not an image", None, false).err().unwrap();
        assert!(err.to_string().contains("Validação"));
        let err = prepare_avatar(&bytes[..24], None, true).err().unwrap();
        assert!(err.to_string().contains("Processamento"));
        let err = prepare_avatar(&png(4097, 1), None, false).err().unwrap();
        assert!(err.to_string().contains("Processamento"));
    }

    #[tokio::test]
    async fn downloads_report_http_and_byte_limit_failures() {
        let bytes = png(512, 512);
        let response: reqwest::Response = http::Response::builder()
            .status(200)
            .header("content-type", "image/png")
            .body(bytes.clone())
            .unwrap()
            .into();
        assert_eq!(
            crate::services::attachments::read_response(response, MAX_AVATAR_BYTES)
                .await
                .unwrap()
                .bytes,
            bytes
        );
        assert!(
            crate::services::attachments::read_response(
                mock(404, "private signed URL"),
                MAX_AVATAR_BYTES
            )
            .await
            .unwrap_err()
            .to_string()
            .contains("HTTP 404")
        );
        let response: reqwest::Response = http::Response::builder()
            .status(200)
            .body(vec![0; MAX_AVATAR_BYTES + 1])
            .unwrap()
            .into();
        assert!(
            crate::services::attachments::read_response(response, MAX_AVATAR_BYTES)
                .await
                .unwrap_err()
                .to_string()
                .contains("acima do limite")
        );
    }

    #[tokio::test]
    async fn discord_error_details_are_allowlisted_and_redacted() {
        let error = parse_response(mock(400, r#"{"code":50035,"message":"SECRET_BASE64_TOKEN","errors":{"avatar":{"_errors":[{"message":"SECRET_BASE64_TOKEN"}]},"SECRET_BASE64_TOKEN":{}}}"#)).await.err().unwrap().to_string();
        assert!(error.contains("50035") && error.contains("avatar") && error.contains("400"));
        assert!(!error.contains("SECRET"));
        assert!(parse_response(mock(200, "invalid")).await.is_err());
    }

    type ApiRecords = Vec<(String, serde_json::Value)>;
    fn mock_api(status: u16, body: &'static str) -> (String, std::thread::JoinHandle<ApiRecords>) {
        mock_profile_api(vec![(status, body)])
    }
    /// Real reqwest serialization and TCP exchange, restricted to a local test listener.
    fn mock_profile_api(
        responses: Vec<(u16, &'static str)>,
    ) -> (String, std::thread::JoinHandle<ApiRecords>) {
        use std::io::Write;
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let handle = std::thread::spawn(move || {
            let mut records = vec![];
            for (status, body) in responses {
                let (mut socket, _) = listener.accept().unwrap();
                socket
                    .set_read_timeout(Some(Duration::from_secs(10)))
                    .unwrap();
                let mut request = Vec::new();
                let (headers, payload) = loop {
                    let mut buffer = [0; 8192];
                    let count = socket.read(&mut buffer).unwrap();
                    assert!(count > 0 && request.len() < 100_000);
                    request.extend_from_slice(&buffer[..count]);
                    if let Some(index) = request.windows(4).position(|w| w == b"\r\n\r\n") {
                        let headers = String::from_utf8(request[..index].to_vec()).unwrap();
                        let length: usize = headers
                            .lines()
                            .find_map(|line| {
                                line.to_lowercase()
                                    .strip_prefix("content-length:")
                                    .map(|v| v.trim().parse().unwrap())
                            })
                            .unwrap_or(0);
                        if request.len() >= index + 4 + length {
                            break (
                                headers,
                                if length == 0 {
                                    serde_json::Value::Null
                                } else {
                                    serde_json::from_slice(&request[index + 4..index + 4 + length])
                                        .unwrap()
                                },
                            );
                        }
                    }
                };
                write!(socket, "HTTP/1.1 {status} OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
                records.push((headers, payload));
            }
            records
        });
        (format!("http://{address}/api/v10"), handle)
    }

    #[tokio::test]
    async fn revert_persists_only_after_http_success_and_never_changes_another_guild() {
        use crate::services::storage::{State, Store};
        for status in [200, 400, 403] {
            let dir = tempfile::tempdir().unwrap();
            let store = Store::open(dir.path().into()).unwrap();
            let gid = s::GuildId::new(1);
            let mut state = State::default();
            state.profile.nickname = Some("Custom".into());
            state.profile.bio = Some("Custom bio".into());
            state.profile.avatar = Some(image_data(&png(512, 512)).unwrap());
            store.write(gid, &state).unwrap();
            store.write(s::GuildId::new(2), &state).unwrap();
            let other = std::fs::read(dir.path().join("guilds/2/state.json")).unwrap();
            let original = std::fs::read(dir.path().join("guilds/1/state.json")).unwrap();
            let response = if status == 200 {
                r#"{"nick":"TOML","bio":"Default","avatar":null}"#
            } else {
                r#"{"code":50035,"errors":{"avatar":{}}}"#
            };
            let (root, server) = mock_profile_api(vec![
                (
                    200,
                    r#"{"nick":"Custom","bio":"Custom bio","avatar":"old_hash"}"#,
                ),
                (status, response),
            ]);
            let mut client = Client::new().unwrap();
            client.api_root = root;
            let desired = revert(&ProfileConfig {
                nickname: Some("TOML".into()),
                bio: Some("Default".into()),
                ..Default::default()
            });
            let result = apply(
                &client,
                "Bot TEST_TOKEN",
                &store,
                Operation {
                    guild: gid,
                    bot: s::UserId::new(9),
                    desired,
                    revert: true,
                },
            )
            .await;
            let records = server.join().unwrap();
            assert!(records[0].0.starts_with("GET /api/v10/guilds/1/members/9"));
            assert_eq!(
                records[1].1,
                serde_json::json!({"nick":"TOML","bio":"Default","avatar":null})
            );
            assert_eq!(result.is_ok(), status == 200);
            if status == 200 {
                let saved = store.read(gid).unwrap();
                assert!(
                    saved.profile.nickname.is_none()
                        && saved.profile.bio.is_none()
                        && saved.profile.avatar.is_none()
                );
            } else {
                assert_eq!(
                    std::fs::read(dir.path().join("guilds/1/state.json")).unwrap(),
                    original
                );
            }
            assert_eq!(
                std::fs::read(dir.path().join("guilds/2/state.json")).unwrap(),
                other
            );
        }
    }
    #[tokio::test]
    async fn nickname_bio_and_avatar_updates_send_only_requested_fields_and_persist_success() {
        use crate::services::storage::{State, Store};
        for desired in [
            Patch {
                nick: Some("New".into()),
                ..Default::default()
            },
            Patch {
                bio: Some("New bio".into()),
                ..Default::default()
            },
            Patch {
                avatar: Some(image_data(&png(1254, 1254)).unwrap()),
                ..Default::default()
            },
        ] {
            let dir = tempfile::tempdir().unwrap();
            let store = Store::open(dir.path().into()).unwrap();
            let gid = s::GuildId::new(1);
            let mut state = State::default();
            state.profile.nickname = Some("Old".into());
            store.write(gid, &state).unwrap();
            let (root, server) = mock_profile_api(vec![
                (200, r#"{"nick":"Old","bio":"Old bio","avatar":null}"#),
                (
                    200,
                    r#"{"nick":"New","bio":"New bio","avatar":"confirmed_hash"}"#,
                ),
            ]);
            let mut client = Client::new().unwrap();
            client.api_root = root;
            assert!(
                apply(
                    &client,
                    "Bot TEST_TOKEN",
                    &store,
                    Operation {
                        guild: gid,
                        bot: s::UserId::new(9),
                        desired: desired.clone(),
                        revert: false
                    }
                )
                .await
                .unwrap()
            );
            let records = server.join().unwrap();
            assert_eq!(records[1].1, serde_json::to_value(&desired).unwrap());
            assert_eq!(records[1].1.as_object().unwrap().len(), 1);
            let saved = store.read(gid).unwrap();
            if desired.nick.is_some() {
                assert_eq!(saved.profile.nickname, desired.nick);
            }
            if desired.bio.is_some() {
                assert_eq!(saved.profile.bio, desired.bio);
                assert_eq!(saved.profile.nickname.as_deref(), Some("Old"));
            }
            if desired.avatar.is_some() {
                assert_eq!(saved.profile.avatar, desired.avatar);
                assert_eq!(
                    saved.profile.applied_avatar_hash.as_deref(),
                    Some("confirmed_hash")
                );
            }
        }
    }

    #[tokio::test]
    async fn avatar_patch_uses_current_guild_route_and_requires_confirmed_success() {
        let bytes = png(512, 512);
        let patch = Patch {
            avatar: Some(image_data(&bytes).unwrap()),
            ..Default::default()
        };
        for (status, body, success) in [
            (200, r#"{"avatar":"guild-avatar-hash"}"#, true),
            (200, r#"{"avatar":null}"#, false),
            (400, r#"{"code":50035,"errors":{"avatar":{}}}"#, false),
            (401, r#"{"code":0}"#, false),
            (403, r#"{"code":50013}"#, false),
            (500, r#"{}"#, false),
        ] {
            let (root, request) = mock_api(status, body);
            let mut client = Client::new().unwrap();
            client.api_root = root;
            let result = client
                .update("Bot TEST_TOKEN", s::GuildId::new(42), &patch)
                .await;
            assert_eq!(result.is_ok(), success);
            let (headers, payload) = request.join().unwrap().remove(0);
            assert!(headers.starts_with("PATCH /api/v10/guilds/42/members/@me HTTP/1.1"));
            assert!(
                headers
                    .to_lowercase()
                    .contains("authorization: bot test_token")
            );
            assert!(
                headers
                    .to_lowercase()
                    .contains("content-type: application/json")
            );
            assert_eq!(payload, serde_json::to_value(&patch).unwrap());
            assert_eq!(payload.as_object().unwrap().len(), 1);
            if let Ok(profile) = result {
                assert_eq!(profile.avatar.as_deref(), Some("guild-avatar-hash"));
            }
        }
    }
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
            assert!(error.to_string().starts_with(error_message(code)));
            assert!(error.to_string().contains(&format!("status {code}")));
        }
    }
    #[tokio::test]
    async fn rate_limits_block_repeated_calls_without_network() {
        let client = Client::new().unwrap();
        let guild = s::GuildId::new(1);
        assert!(
            client
                .response(
                    guild,
                    mock(429, r#"{"retry_after":60,"global":false}"#),
                    "PATCH /guilds/{guild}/members/@me"
                )
                .await
                .is_err()
        );
        assert!(client.check_cooldown(guild).await.is_err());
        assert!(client.check_cooldown(s::GuildId::new(2)).await.is_ok());
        assert!(
            client
                .response(
                    guild,
                    mock(429, r#"{"retry_after":60,"global":true}"#),
                    "PATCH /guilds/{guild}/members/@me"
                )
                .await
                .is_err()
        );
        assert!(client.check_cooldown(s::GuildId::new(2)).await.is_err());
    }
}
