//! Migração não destrutiva: planeja e valida todos os destinos antes de gravar.
use super::*;
use anyhow::Context;
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};

fn create_new(path: &Path, bytes: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut file = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(path)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    Ok(())
}
pub fn migrate(root: &Path, templates: &Path) -> Result<usize> {
    let global_path = root.join("default.toml");
    let source = fs::read_to_string(&global_path)?;
    let mut global: toml::Value = toml::from_str(&source)?;
    ensure!(
        global.get("guilds").is_none(),
        "Registro central já existe; migração não executada"
    );
    let parsed: GlobalConfig = toml::from_str(&source)?;
    let mut registry = toml::Table::new();
    let mut ids = std::collections::HashSet::new();
    let mut folders = std::collections::HashSet::new();
    let mut writes: Vec<(PathBuf, Vec<u8>)> = vec![];
    let mut sources = fs::read_dir(root.join("guilds"))?
        .map(|e| e.map(|e| e.path()))
        .collect::<std::io::Result<Vec<_>>>()?;
    sources.sort();
    for path in sources {
        if !path.is_file() || path.extension().is_none_or(|ext| ext != "toml") {
            continue;
        }
        let text = fs::read_to_string(&path)?;
        let guild: GuildConfig = toml::from_str(&text)
            .with_context(|| format!("Legado inválido: {}", path.display()))?;
        guild.validate(parsed.modules)?;
        let alias = guild
            .alias
            .clone()
            .unwrap_or_else(|| format!("guild-{}", guild.guild_id));
        let name = guild
            .name
            .clone()
            .unwrap_or_else(|| path.file_stem().unwrap().to_string_lossy().into_owned());
        ensure!(
            registry::valid_alias(&alias) && !registry.contains_key(&alias),
            "Alias inválido/duplicado"
        );
        ensure!(ids.insert(guild.guild_id), "Guild ID duplicado");
        ensure!(
            folders.insert(registry::folder_key(&name)?),
            "Pasta equivalente/duplicada"
        );
        let folder = root.join("guilds").join(&name);
        ensure!(!folder.exists(), "Destino já existe: {}", folder.display());
        registry.insert(
            alias,
            toml::Value::try_from(GuildRegistration {
                id: guild.guild_id,
                name,
                enabled: guild.enabled,
            })?,
        );
        let mut local: toml::Value = toml::from_str(&text)?;
        let table = local
            .as_table_mut()
            .context("Configuração deve ser tabela")?;
        for key in ["guild_id", "alias", "name", "enabled", "templates"] {
            table.remove(key);
        }
        // Preserve the old effective module settings rather than applying new defaults.
        table.insert(
            "modules".into(),
            toml::Value::try_from(guild.modules.unwrap_or(parsed.modules))?,
        );
        writes.push((
            folder.join("config.toml"),
            toml::to_string_pretty(&local)?.into_bytes(),
        ));
        for kind in ["rules", "announcements", "links"] {
            let doc =
                crate::services::storage::template(templates, &guild, kind, &Default::default())?;
            let default_path = root.join("guilds/defaults").join(format!("{kind}.toml"));
            let default_value = loader::optional(&default_path)?;
            let default_doc: crate::services::content::Document = default_value.try_into()?;
            if serde_json::to_value(&doc)? != serde_json::to_value(default_doc)? {
                writes.push((
                    folder.join(format!("{kind}.toml")),
                    toml::to_string_pretty(&doc)?.into_bytes(),
                ));
            }
        }
    }
    ensure!(
        !registry.is_empty(),
        "Nenhuma configuração legada encontrada"
    );
    let count = registry.len();
    global
        .as_table_mut()
        .unwrap()
        .insert("guilds".into(), toml::Value::Table(registry));
    let updated = toml::to_string_pretty(&global)?;
    let backup = root.join("default.toml.pre-multiguild");
    ensure!(
        !backup.exists(),
        "Backup já existe; revise uma migração anterior"
    );
    for (path, _) in &writes {
        ensure!(!path.exists(), "Destino já existe: {}", path.display());
    }
    // Validate the complete prospective tree before touching the user's files.
    let stage = tempfile::tempdir()?;
    fs::create_dir_all(stage.path().join("guilds/defaults"))?;
    if root.join("guilds/defaults").exists() {
        for entry in fs::read_dir(root.join("guilds/defaults"))? {
            let entry = entry?;
            if entry.path().is_file() {
                fs::copy(
                    entry.path(),
                    stage.path().join("guilds/defaults").join(entry.file_name()),
                )?;
            }
        }
    }
    for (path, bytes) in &writes {
        create_new(&stage.path().join(path.strip_prefix(root)?), bytes)?;
    }
    fs::write(stage.path().join("default.toml"), &updated)?;
    load(stage.path())?;
    create_new(&backup, source.as_bytes())?;
    for (path, bytes) in &writes {
        create_new(path, bytes)?;
    }
    let mut temp = tempfile::NamedTempFile::new_in(root)?;
    temp.write_all(updated.as_bytes())?;
    temp.as_file().sync_all()?;
    temp.persist(global_path).map_err(|e| e.error)?;
    Ok(count)
}
