use super::*;
use anyhow::Context;
use std::{fs, path::Path};

pub fn load(root: &Path) -> Result<Config> {
    let path = root.join("default.toml");
    let global: GlobalConfig = parse(&path)?;
    global.validate().context("configuração global inválida")?;
    let mut guilds = HashMap::new();
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
        let guild: GuildConfig = parse(&path)?;
        guild
            .validate(global.modules)
            .with_context(|| format!("configuração inválida: {}", path.display()))?;
        let id = guild.guild_id;
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
