use super::content::Document;
use anyhow::{Context, Result, ensure};
use poise::serenity_prelude::{ChannelId, GuildId, MessageId};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Publication {
    pub channel: ChannelId,
    pub message: MessageId,
}
#[derive(Clone, Default, Serialize, Deserialize)]
pub struct State {
    #[serde(default)]
    pub profile: crate::services::profile::ProfileState,
    pub revision: u64,
    pub documents: HashMap<String, Document>,
    pub publications: HashMap<String, Publication>,
    pub pending: Option<String>,
}
pub struct Store {
    root: PathBuf,
    _lock: File,
    pub gate: tokio::sync::Mutex<()>,
}
impl Store {
    pub fn open(root: PathBuf) -> Result<Self> {
        fs::create_dir_all(&root)?;
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(root.join(".lock"))?;
        lock.try_lock()
            .context("Outro processo utiliza o diretório de dados")?;
        Ok(Self {
            root,
            _lock: lock,
            gate: tokio::sync::Mutex::new(()),
        })
    }
    fn path(&self, guild: GuildId) -> PathBuf {
        self.root
            .join("guilds")
            .join(guild.to_string())
            .join("state.json")
    }
    pub fn read(&self, guild: GuildId) -> Result<State> {
        let path = self.path(guild);
        match fs::read(&path) {
            Ok(data) => serde_json::from_slice(&data)
                .context("Estado JSON inválido; restaure o backup antes de continuar"),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(State::default()),
            Err(e) => Err(e.into()),
        }
    }
    // Chamadores devem manter gate adquirido durante read/modify/write e efeitos HTTP.
    pub fn write(&self, guild: GuildId, state: &State) -> Result<()> {
        let path = self.path(guild);
        let parent = path.parent().context("Diretório inválido")?;
        fs::create_dir_all(parent)?;
        let mut file = tempfile::NamedTempFile::new_in(parent)?;
        file.write_all(&serde_json::to_vec_pretty(state)?)?;
        file.as_file().sync_all()?;
        file.persist(path).map_err(|e| e.error)?;
        Ok(())
    }
}

pub fn template(
    root: &Path,
    guild: &crate::config::GuildConfig,
    kind: &str,
    state: &State,
) -> Result<Document> {
    if let Some(doc) = state.documents.get(kind) {
        return Ok(doc.clone());
    }
    if let Some(doc) = guild.contents.get(kind) {
        return Ok(doc.clone());
    }
    let name = guild
        .templates
        .get(kind)
        .cloned()
        .unwrap_or_else(|| format!("{kind}/default"));
    ensure!(
        name.split('/').count() == 2 && name.split('/').all(crate::config::registry::valid_alias),
        "Template inválido"
    );
    let mut value = toml::Value::try_from(Document {
        title: kind.to_string(),
        color: 0x5865f2,
        ..Default::default()
    })?;
    for path in [
        root.join("global").join(format!("{name}.toml")),
        root.join("guilds")
            .join(guild.guild_id.to_string())
            .join(format!("{name}.toml")),
    ] {
        match fs::read_to_string(path) {
            Ok(text) => {
                let overlay: toml::Value = toml::from_str(&text)?;
                let table = overlay.as_table().context("Template deve ser tabela")?;
                for (key, field) in table {
                    value
                        .as_table_mut()
                        .unwrap()
                        .insert(key.clone(), field.clone());
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
    }
    let doc: Document = value.try_into()?;
    doc.embeds(kind)?;
    Ok(doc)
}
