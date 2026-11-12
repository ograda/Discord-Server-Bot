//! Rules/Links only: validated snapshots, explicit reload and atomic TOML writes.
use crate::{
    config::{self, Config, GuildConfig},
    services::content::{Document, Entry},
};
use anyhow::{Context, Result, ensure};
use poise::serenity_prelude::GuildId;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    fs,
    io::Write,
    path::{Path, PathBuf},
};
use tokio::sync::RwLock;

fn color() -> u32 {
    0x5865f2
}
fn links_channel() -> String {
    "links".into()
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Board {
    pub id: String,
    pub title: String,
    pub description: String,
    pub channel: String,
    pub color: u32,
    pub thumbnail: String,
    pub image: String,
    pub footer: String,
    pub text: String,
    #[serde(alias = "entries")]
    pub links: Vec<Entry>,
}
impl Default for Board {
    fn default() -> Self {
        Self {
            id: "default".into(),
            title: "Links".into(),
            description: String::new(),
            channel: links_channel(),
            color: color(),
            thumbnail: String::new(),
            image: String::new(),
            footer: String::new(),
            text: String::new(),
            links: vec![],
        }
    }
}
impl Board {
    pub fn from_document(doc: Document) -> Self {
        Self {
            title: doc.title,
            description: doc.description,
            color: doc.color,
            thumbnail: doc.thumbnail,
            image: doc.image,
            footer: doc.footer,
            text: doc.text,
            links: doc.entries,
            ..Default::default()
        }
    }
    pub fn document(&self) -> Document {
        Document {
            title: self.title.clone(),
            description: self.description.clone(),
            color: self.color,
            thumbnail: self.thumbnail.clone(),
            image: self.image.clone(),
            footer: self.footer.clone(),
            text: self.text.clone(),
            entries: self.links.clone(),
        }
    }
    pub fn publication_key(&self) -> String {
        if self.id == "default" {
            "links".into()
        } else {
            format!("links:{}", self.id)
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LinksFile {
    pub boards: Vec<Board>,
}
impl LinksFile {
    pub fn validate(&self, guild: &GuildConfig) -> Result<()> {
        let mut ids = std::collections::HashSet::new();
        for board in &self.boards {
            ensure!(
                config::registry::valid_alias(&board.id) && ids.insert(&board.id),
                "Board ID inválido ou duplicado"
            );
            ensure!(
                config::registry::valid_alias(&board.channel),
                "Alias de canal do quadro inválido"
            );
            if guild.modules.is_some_and(|m| m.links) {
                guild.channel(&board.channel)?;
            }
            board.document().embeds("links")?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct Snapshot {
    pub rules: Document,
    pub links: LinksFile,
    pub sources: HashMap<String, String>,
    pub revision: u64,
}
pub struct Catalog {
    snapshots: RwLock<HashMap<GuildId, Snapshot>>,
}
impl Catalog {
    pub fn new(config: &Config) -> Self {
        Self {
            snapshots: RwLock::new(
                config
                    .guilds
                    .iter()
                    .map(|(id, g)| {
                        (
                            *id,
                            Snapshot {
                                rules: g.contents.get("rules").cloned().unwrap_or_default(),
                                links: LinksFile {
                                    boards: if let Some(boards) = &g.link_boards {
                                        boards.clone()
                                    } else {
                                        vec![Board::from_document(
                                            g.contents.get("links").cloned().unwrap_or_else(|| {
                                                Document {
                                                    title: "Links".into(),
                                                    color: color(),
                                                    ..Default::default()
                                                }
                                            }),
                                        )]
                                    },
                                },
                                sources: g.content_sources.clone(),
                                revision: 0,
                            },
                        )
                    })
                    .collect(),
            ),
        }
    }
    pub async fn snapshot(&self, guild: GuildId) -> Result<Snapshot> {
        self.snapshots
            .read()
            .await
            .get(&guild)
            .cloned()
            .context("Conteúdo da guilda não carregado")
    }
    /// Caller holds Store.gate to serialize reload/reset/add with publication confirmation.
    pub async fn reload(&self, root: &Path, guild: &GuildConfig, kind: &str) -> Result<String> {
        let (value, source) = read_content(root, guild, kind)?;
        let mut snapshots = self.snapshots.write().await;
        let snapshot = snapshots
            .get_mut(&guild.guild_id)
            .context("Guilda não carregada")?;
        replace(snapshot, guild, kind, value)?;
        snapshot.sources.insert(kind.into(), source.clone());
        snapshot.revision += 1;
        Ok(source)
    }
    pub async fn reset(
        &self,
        root: &Path,
        guild: &GuildConfig,
        kind: &str,
    ) -> Result<(String, Option<PathBuf>)> {
        let (defaults, target) = paths(root, guild, kind)?;
        let bytes = fs::read_to_string(&defaults)
            .with_context(|| format!("Default indisponível: {}", defaults.display()))?;
        let value: toml::Value =
            toml::from_str(&bytes).context("Default TOML inválido; nada restaurado")?;
        let mut candidate = self.snapshot(guild.guild_id).await?;
        replace(&mut candidate, guild, kind, value)?;
        let backup = atomic_write(&target, bytes.as_bytes(), None)?;
        let source = target.display().to_string();
        candidate.sources.insert(kind.into(), source.clone());
        candidate.revision += 1;
        self.snapshots
            .write()
            .await
            .insert(guild.guild_id, candidate);
        Ok((source, backup))
    }
    pub async fn add_link(
        &self,
        root: &Path,
        guild: &GuildConfig,
        board_id: &str,
        entry: Entry,
    ) -> Result<(String, Option<PathBuf>)> {
        self.change_links(root, guild, board_id, LinkChange::Add(entry))
            .await
    }
    pub async fn remove_link(
        &self,
        root: &Path,
        guild: &GuildConfig,
        board_id: &str,
        expected: Entry,
    ) -> Result<(String, Option<PathBuf>)> {
        self.change_links(root, guild, board_id, LinkChange::Remove(expected))
            .await
    }
    pub async fn edit_board(
        &self,
        root: &Path,
        guild: &GuildConfig,
        board_id: &str,
        expected: Document,
        updated: Document,
    ) -> Result<(String, Option<PathBuf>)> {
        self.change_links(
            root,
            guild,
            board_id,
            LinkChange::Board { expected, updated },
        )
        .await
    }
    async fn change_links(
        &self,
        root: &Path,
        guild: &GuildConfig,
        board_id: &str,
        change: LinkChange,
    ) -> Result<(String, Option<PathBuf>)> {
        let (defaults, target) = paths(root, guild, "links")?;
        let original = read_optional_bytes(&target)?;
        let (value, _) = read_content(root, guild, "links")?;
        let mut effective = parse_links(value)?;
        let board = effective
            .boards
            .iter_mut()
            .find(|b| b.id == board_id)
            .context("Quadro não encontrado no TOML; execute Update e selecione novamente")?;
        let previous = board.clone();
        match &change {
            LinkChange::Add(entry) => {
                entry.validate_link()?;
                ensure!(
                    !board.links.iter().any(|v| v.id == entry.id),
                    "Link ID já existe neste quadro"
                );
                board.links.push(entry.clone());
            }
            LinkChange::Remove(expected) => {
                let entry = board
                    .links
                    .iter()
                    .find(|v| v.id == expected.id)
                    .context("Link já removido do TOML; recarregue o catálogo")?;
                ensure!(
                    entry == expected,
                    "Link alterado desde a seleção; confira e confirme novamente"
                );
                board.links.retain(|v| v.id != expected.id);
            }
            LinkChange::Board { expected, updated } => {
                ensure!(
                    board.document() == *expected,
                    "Quadro alterado no disco; execute Update antes de editar novamente"
                );
                ensure!(
                    expected.entries == updated.entries,
                    "A edição do quadro não pode substituir seu catálogo de links"
                );
                let id = board.id.clone();
                let channel = board.channel.clone();
                *board = Board::from_document(updated.clone());
                board.id = id;
                board.channel = channel;
            }
        }
        effective.validate(guild)?;
        let raw = match &original {
            Some(bytes) => String::from_utf8(bytes.clone()).context("links.toml não é UTF-8")?,
            None => match fs::read_to_string(&defaults) {
                Ok(text) => text,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
                Err(e) => return Err(e.into()),
            },
        };
        let mut edit: toml_edit::DocumentMut = raw
            .parse()
            .context("links.toml inválido; nenhuma gravação realizada")?;
        if let Some(boards) = edit
            .get_mut("boards")
            .and_then(|v| v.as_array_of_tables_mut())
        {
            let table = boards
                .iter_mut()
                .find(|v| v.get("id").and_then(|id| id.as_str()) == Some(board_id))
                .context("Quadro não encontrado no arquivo específico")?;
            let key = if table.contains_key("entries") {
                "entries"
            } else {
                "links"
            };
            change_table(table, key, &previous, &change)?;
        } else if board_id == "default" && edit.get("boards").is_none() {
            change_table(edit.as_table_mut(), "entries", &previous, &change)?;
        } else {
            let serialized = toml_edit::ser::to_document(&effective)?;
            for key in [
                "title",
                "description",
                "color",
                "thumbnail",
                "image",
                "footer",
                "text",
                "entries",
            ] {
                edit.remove(key);
            }
            edit["boards"] = serialized["boards"].clone();
        }
        let updated = edit.to_string();
        let mut defaults_value = config::loader::optional(&defaults)?;
        merge_content(&mut defaults_value, toml::from_str(&updated)?, "links");
        let links = parse_links(defaults_value)?;
        links.validate(guild)?;
        let backup = atomic_write(&target, updated.as_bytes(), Some(original.as_deref()))?;
        let mut snapshots = self.snapshots.write().await;
        let snapshot = snapshots
            .get_mut(&guild.guild_id)
            .context("Guilda não carregada")?;
        snapshot.links = links;
        snapshot.revision += 1;
        let source = target.display().to_string();
        snapshot.sources.insert("links".into(), source.clone());
        Ok((source, backup))
    }
}
enum LinkChange {
    Add(Entry),
    Remove(Entry),
    Board {
        expected: Document,
        updated: Document,
    },
}
fn change_table(
    table: &mut toml_edit::Table,
    key: &str,
    previous: &Board,
    change: &LinkChange,
) -> Result<()> {
    if let LinkChange::Board { updated, .. } = change {
        let serialized = toml_edit::ser::to_document(updated)?;
        for key in [
            "title",
            "description",
            "color",
            "thumbnail",
            "image",
            "footer",
            "text",
        ] {
            table[key] = serialized[key].clone();
        }
        return Ok(());
    }
    // Missing arrays inherit defaults. Materialize before editing so inherited links survive.
    let mut entries = if let Some(array) = table.get(key).and_then(|v| v.as_array_of_tables()) {
        array.clone()
    } else if let Some(array) = table.get(key).and_then(|v| v.as_array()) {
        let mut tables = toml_edit::ArrayOfTables::new();
        for value in array {
            tables.push(
                value
                    .as_inline_table()
                    .context("Entrada de link TOML inválida")?
                    .clone()
                    .into_table(),
            );
        }
        tables
    } else {
        let mut tables = toml_edit::ArrayOfTables::new();
        for entry in &previous.links {
            tables.push(toml_edit::ser::to_document(entry)?.as_table().clone());
        }
        tables
    };
    match change {
        LinkChange::Add(entry) => {
            entries.push(toml_edit::ser::to_document(entry)?.as_table().clone())
        }
        LinkChange::Remove(entry) => {
            let mut retained = toml_edit::ArrayOfTables::new();
            for table in entries
                .iter()
                .filter(|t| t.get("id").and_then(|v| v.as_str()) != Some(entry.id.as_str()))
            {
                retained.push(table.clone());
            }
            entries = retained;
        }
        LinkChange::Board { .. } => unreachable!(),
    }
    // An explicit empty array must shadow inherited entries after removal of the last link.
    table[key] = if entries.is_empty() {
        toml_edit::value(toml_edit::Array::new())
    } else {
        toml_edit::Item::ArrayOfTables(entries)
    };
    Ok(())
}
fn replace(
    snapshot: &mut Snapshot,
    guild: &GuildConfig,
    kind: &str,
    value: toml::Value,
) -> Result<()> {
    match kind {
        "rules" => {
            let doc: Document = value.try_into()?;
            doc.embeds("rules")?;
            snapshot.rules = doc;
        }
        "links" => {
            let links = parse_links(value)?;
            links.validate(guild)?;
            snapshot.links = links;
        }
        _ => anyhow::bail!("Recarregamento limitado a Rules e Links"),
    }
    Ok(())
}
pub fn parse_links(value: toml::Value) -> Result<LinksFile> {
    if value.get("boards").is_some() {
        Ok(value.try_into()?)
    } else {
        Ok(LinksFile {
            boards: vec![Board::from_document(value.try_into()?)],
        })
    }
}
fn merge_content(base: &mut toml::Value, overlay: toml::Value, kind: &str) {
    if kind == "links"
        && overlay.get("boards").is_some() != base.get("boards").is_some()
        && overlay.as_table().is_some_and(|t| !t.is_empty())
    {
        *base = overlay;
    } else {
        config::loader::merge(base, overlay);
    }
}
pub fn read_content(root: &Path, guild: &GuildConfig, kind: &str) -> Result<(toml::Value, String)> {
    let (defaults, specific) = paths(root, guild, kind)?;
    let mut value = if kind == "links" {
        toml::Value::Table(Default::default())
    } else {
        toml::Value::try_from(Document {
            title: kind.into(),
            color: color(),
            ..Default::default()
        })?
    };
    let mut sources = vec![];
    for path in [defaults, specific] {
        if path.is_file() {
            sources.push(path.display().to_string());
        }
        merge_content(&mut value, config::loader::optional(&path)?, kind);
    }
    if kind == "links" && value.as_table().is_some_and(|t| t.is_empty()) {
        value = toml::Value::try_from(Document {
            title: "Links".into(),
            color: color(),
            ..Default::default()
        })?;
    }
    Ok((
        value,
        if sources.is_empty() {
            "valores internos".into()
        } else {
            sources.join(" → ")
        },
    ))
}
pub fn paths(root: &Path, guild: &GuildConfig, kind: &str) -> Result<(PathBuf, PathBuf)> {
    ensure!(
        ["rules", "links", "config", "announcements"].contains(&kind),
        "Arquivo de configuração não permitido"
    );
    let name = guild
        .name
        .as_deref()
        .context("Operação exige registro multiguild; migre a configuração legada")?;
    config::registry::folder_key(name)?;
    let defaults = root.join("guilds/defaults").join(format!("{kind}.toml"));
    let target = root.join("guilds").join(name).join(format!("{kind}.toml"));
    for path in [&defaults, &target] {
        contained(root, path)?;
        for check in [
            root.join("guilds"),
            path.parent().unwrap().to_path_buf(),
            path.clone(),
        ] {
            if let Ok(metadata) = fs::symlink_metadata(&check) {
                ensure!(
                    !metadata.file_type().is_symlink(),
                    "Links simbólicos/junções não são permitidos nos arquivos de conteúdo por guilda"
                );
            }
        }
    }
    Ok((defaults, target))
}
fn contained(root: &Path, path: &Path) -> Result<()> {
    let root = root.canonicalize()?;
    let mut existing = path;
    while !existing.exists() {
        existing = existing.parent().context("Caminho inválido")?;
    }
    ensure!(
        existing.canonicalize()?.starts_with(root),
        "Caminho fora da configuração autorizada"
    );
    Ok(())
}
fn read_optional_bytes(path: &Path) -> Result<Option<Vec<u8>>> {
    match fs::read(path) {
        Ok(v) => Ok(Some(v)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.into()),
    }
}
pub fn atomic_write(
    path: &Path,
    bytes: &[u8],
    expected: Option<Option<&[u8]>>,
) -> Result<Option<PathBuf>> {
    let parent = path.parent().context("Diretório inválido")?;
    fs::create_dir_all(parent)?;
    let old = read_optional_bytes(path)?;
    if let Some(expected) = expected {
        ensure!(
            old.as_deref() == expected,
            "Arquivo alterado durante a operação; tente novamente"
        );
    }
    let backup = if let Some(old) = old {
        let prefix = format!("{}.backup-", path.file_name().unwrap().to_string_lossy());
        let mut file = tempfile::Builder::new()
            .prefix(&prefix)
            .suffix(".bak")
            .tempfile_in(parent)?;
        file.write_all(&old)?;
        file.as_file().sync_all()?;
        Some(file.keep().map_err(|e| e.error)?.1)
    } else {
        None
    };
    let mut temp = tempfile::NamedTempFile::new_in(parent)?;
    temp.write_all(bytes)?;
    temp.as_file().sync_all()?;
    temp.persist(path).map_err(|e| e.error)?;
    Ok(backup)
}
