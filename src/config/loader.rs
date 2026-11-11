use super::*;
use anyhow::Context;
use std::{fs, path::Path};

pub fn load(root: &Path) -> Result<Config> {
    let path = root.join("default.toml");
    let global: GlobalConfig = parse(&path)?;
    global.validate().context("configuração global inválida")?;
    if global.guilds.is_some() {
        return load_registered(root, global);
    }
    let mut guilds = HashMap::new();
    let mut aliases = std::collections::HashSet::new();
    let dir = root.join("guilds");
    let mut paths = fs::read_dir(&dir)
        .with_context(|| format!("não foi possível ler {}", dir.display()))?
        .map(|e| e.map(|e| e.path()))
        .collect::<std::io::Result<Vec<_>>>()?;
    paths.sort();
    for path in paths {
        if !path.is_file() || path.extension().is_none_or(|e| e != "toml") {
            continue;
        }
        let mut guild: GuildConfig = parse(&path)?;
        ensure!(
            guild.profile.avatar.is_none(),
            "Avatar local exige configuração multiguild atual; execute a migração antes de habilitar assets de perfil"
        );
        cache_legacy(&mut guild)?;
        guild
            .validate(global.modules)
            .with_context(|| format!("configuração inválida: {}", path.display()))?;
        let id = guild.guild_id;
        if let Some(alias) = &guild.alias {
            ensure!(
                aliases.insert(alias.clone()),
                "alias de servidor duplicado: {alias}"
            );
        }
        ensure!(
            guilds.insert(id, guild).is_none(),
            "guild_id duplicado: {id} ({})",
            path.display()
        );
    }
    ensure!(
        !guilds.is_empty(),
        "nenhum servidor autorizado; crie config/guilds/servidor.toml a partir do exemplo"
    );
    Ok(Config { global, guilds })
}
fn parse<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T> {
    let content = fs::read_to_string(path)
        .with_context(|| format!("não foi possível ler {}", path.display()))?;
    toml::from_str(&content).with_context(|| format!("TOML inválido: {}", path.display()))
}

pub(super) fn merge(base: &mut toml::Value, overlay: toml::Value) {
    match (base, overlay) {
        (toml::Value::Table(base), toml::Value::Table(overlay)) => {
            for (key, value) in overlay {
                if let Some(old) = base.get_mut(&key) {
                    merge(old, value);
                } else {
                    base.insert(key, value);
                }
            }
        }
        (base, overlay) => *base = overlay,
    }
}
pub(super) fn optional(path: &Path) -> Result<toml::Value> {
    match fs::read_to_string(path) {
        Ok(text) => {
            toml::from_str(&text).with_context(|| format!("TOML inválido: {}", path.display()))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            Ok(toml::Value::Table(Default::default()))
        }
        Err(e) => Err(e).with_context(|| format!("Não foi possível ler {}", path.display())),
    }
}
fn cache_legacy(guild: &mut GuildConfig) -> Result<()> {
    let root = std::env::var("TEMPLATE_DIR").unwrap_or_else(|_| "templates".into());
    for kind in ["rules", "announcements", "links"] {
        let doc =
            crate::services::storage::template(Path::new(&root), guild, kind, &Default::default())?;
        guild.contents.insert(kind.into(), doc);
    }
    Ok(())
}
fn load_registered(root: &Path, global: GlobalConfig) -> Result<Config> {
    let defaults = root.join("guilds/defaults");
    let mut ids = std::collections::HashSet::new();
    let mut folders = std::collections::HashSet::new();
    let mut guilds = HashMap::new();
    for (alias, registration) in global.guilds.as_ref().unwrap() {
        ensure!(registry::valid_alias(alias), "Alias inválido: {alias}");
        ensure!(ids.insert(registration.id), "Guild ID duplicado");
        let key = registry::folder_key(&registration.name)
            .with_context(|| format!("Pasta inválida para {alias}"))?;
        ensure!(
            folders.insert(key),
            "Nomes de pasta equivalentes ou duplicados"
        );
        let folder = root.join("guilds").join(&registration.name);
        // Symlinks/junctions must not escape the configuration tree.
        for path in [&defaults, &folder] {
            if path.exists() {
                ensure!(
                    path.canonicalize()?.starts_with(root.canonicalize()?),
                    "Pasta fora do diretório de configuração"
                );
            }
        }
        let mut config = toml::Value::Table(Default::default());
        config
            .as_table_mut()
            .unwrap()
            .insert("modules".into(), toml::Value::try_from(global.modules)?);
        for path in [defaults.join("config.toml"), folder.join("config.toml")] {
            let overlay = optional(&path)?;
            for forbidden in ["guild_id", "id", "alias", "name", "enabled", "templates"] {
                ensure!(
                    overlay.get(forbidden).is_none(),
                    "{forbidden} não permitido em {}",
                    path.display()
                );
            }
            merge(&mut config, overlay);
        }
        config.as_table_mut().unwrap().insert(
            "guild_id".into(),
            toml::Value::String(registration.id.to_string()),
        );
        config
            .as_table_mut()
            .unwrap()
            .insert("alias".into(), toml::Value::String(alias.clone()));
        config.as_table_mut().unwrap().insert(
            "name".into(),
            toml::Value::String(registration.name.clone()),
        );
        config
            .as_table_mut()
            .unwrap()
            .insert("enabled".into(), toml::Value::Boolean(registration.enabled));
        let mut guild: GuildConfig = config
            .try_into()
            .with_context(|| format!("Configuração inválida: {alias}"))?;
        guild.profile.load_avatar(&folder)?;
        let mut commands = toml::Value::Table(Default::default());
        for path in [defaults.join("commands.toml"), folder.join("commands.toml")] {
            merge(&mut commands, optional(&path)?);
        }
        #[derive(Deserialize, Default)]
        #[serde(default, deny_unknown_fields)]
        struct Commands {
            commands: HashMap<String, bool>,
        }
        guild.commands = commands.try_into::<Commands>()?.commands;
        for key in guild.commands.keys() {
            ensure!(
                availability::COMMANDS.contains(&key.as_str()),
                "Comando desconhecido: {key} ({alias})"
            );
        }
        for kind in ["rules", "announcements", "links"] {
            let mut value = toml::Value::try_from(crate::services::content::Document {
                title: kind.into(),
                color: 0x5865f2,
                ..Default::default()
            })?;
            for path in [
                defaults.join(format!("{kind}.toml")),
                folder.join(format!("{kind}.toml")),
            ] {
                merge(&mut value, optional(&path)?);
            }
            let doc: crate::services::content::Document = value
                .try_into()
                .with_context(|| format!("Conteúdo inválido: {alias}/{kind}"))?;
            doc.embeds(kind)
                .with_context(|| format!("Conteúdo inválido: {alias}/{kind}"))?;
            guild.contents.insert(kind.into(), doc);
        }
        if guild.enabled {
            guild.validate(global.modules)?;
        } else {
            guild.validate_registry()?;
        }
        guilds.insert(guild.guild_id, guild);
    }
    Ok(Config { global, guilds })
}
