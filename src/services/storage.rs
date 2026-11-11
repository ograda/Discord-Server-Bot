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
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub archived_documents: Vec<ArchivedDocument>,
    #[serde(default)]
    pub profile: crate::services::profile::ProfileState,
    pub revision: u64,
    pub documents: HashMap<String, Document>,
    pub publications: HashMap<String, Publication>,
    pub pending: Option<String>,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct ArchivedDocument {
    pub key: String,
    pub document: Document,
    pub revision: u64,
}
impl State {
    /// Remove only the rules override. Publication references and other guild content survive.
    pub fn reset_rules(&mut self) -> Result<bool> {
        ensure!(
            self.pending.is_none(),
            "Existe operação pendente; confira o registro antes de resetar as regras"
        );
        let removed = if let Some(document) = self.documents.remove("rules") {
            self.archived_documents.push(ArchivedDocument {
                key: "rules".into(),
                document,
                revision: self.revision,
            });
            true
        } else {
            false
        };
        if removed {
            self.revision += 1;
        }
        Ok(removed)
    }
}

pub fn content_source(guild: &crate::config::GuildConfig, kind: &str, state: &State) -> String {
    if !["rules", "links"].contains(&kind) && state.documents.contains_key(kind) {
        format!(
            "JSON persistido da guilda {} (prioridade sobre TOML)",
            guild.guild_id
        )
    } else {
        guild
            .content_sources
            .get(kind)
            .cloned()
            .unwrap_or_else(|| "templates legados".into())
    }
}

/// A publication is a reference, not a customization. Do not freeze TOML on publish.
pub fn remember_publication_content(
    state: &mut State,
    key: &str,
    doc: &Document,
    baseline: &Document,
) {
    let _ = baseline;
    if key == "rules" || key == "links" || key.starts_with("links:") {
        return;
    }
    state.documents.insert(key.into(), doc.clone());
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
    /// Preserve the exact old JSON before archiving obsolete content overrides.
    pub fn migrate_content(
        &self,
        guild: GuildId,
        first_board: Option<&crate::services::catalog::Board>,
    ) -> Result<bool> {
        let mut state = self.read(guild)?;
        let keys: Vec<_> = state
            .documents
            .keys()
            .filter(|k| k.as_str() == "rules" || k.as_str() == "links" || k.starts_with("links:"))
            .cloned()
            .collect();
        let board_key = first_board.map(|b| b.publication_key());
        let move_links = board_key.as_ref().is_some_and(|key| {
            key != "links"
                && state.publications.contains_key("links")
                && !state.publications.contains_key(key)
        });
        if keys.is_empty() && !move_links {
            return Ok(false);
        }
        ensure!(
            state.pending.is_none(),
            "Migração de conteúdo bloqueada por operação pendente; reconcilie a guilda antes de iniciar"
        );
        let path = self.path(guild);
        let original = fs::read(&path)?;
        let mut backup = tempfile::Builder::new()
            .prefix("state.pre-toml-")
            .suffix(".json.bak")
            .tempfile_in(path.parent().unwrap())?;
        backup.write_all(&original)?;
        backup.as_file().sync_all()?;
        backup.keep().map_err(|e| e.error)?;
        for key in keys {
            let document = state.documents.remove(&key).unwrap();
            state.archived_documents.push(ArchivedDocument {
                key,
                document,
                revision: state.revision,
            });
        }
        if move_links {
            let publication = state.publications.remove("links").unwrap();
            state.publications.insert(board_key.unwrap(), publication);
        }
        state.revision += 1;
        self.write(guild, &state)?;
        tracing::info!(%guild, stage = "migration", "Conteúdo JSON legado preservado em backup e arquivo; TOML passa a ser a fonte");
        Ok(true)
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
    if let Some(doc) = state
        .documents
        .get(kind)
        .filter(|_| !["rules", "links"].contains(&kind))
    {
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
