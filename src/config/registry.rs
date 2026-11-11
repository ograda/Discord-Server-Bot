use super::*;
use anyhow::Context;

pub fn folder_key(name: &str) -> Result<String> {
    ensure!(
        !name.is_empty()
            && name.trim() == name
            && !name.ends_with('.')
            && name.encode_utf16().count() <= 200,
        "Nome de pasta vazio ou inválido"
    );
    ensure!(
        !name
            .chars()
            .any(|c| c.is_control() || "<>:\"/\\|?*".contains(c)),
        "Nome de pasta contém caracteres inválidos"
    );
    let normalizer = icu_normalizer::ComposingNormalizer::new_nfkc();
    let key = normalizer.normalize(&name.to_uppercase()).into_owned();
    ensure!(
        !key.chars()
            .any(|c| c.is_control() || "<>:\"/\\|?*".contains(c))
            && !key.ends_with('.')
            && !key.ends_with(' '),
        "Nome normalizado inválido"
    );
    let base = key.split('.').next().unwrap_or_default();
    ensure!(
        ![
            "", "DEFAULTS", "CON", "PRN", "AUX", "NUL", "CONIN$", "CONOUT$"
        ]
        .contains(&base),
        "Nome de pasta reservado"
    );
    ensure!(
        !(base.len() == 4
            && (base.starts_with("COM") || base.starts_with("LPT"))
            && base.as_bytes()[3].is_ascii_digit()),
        "Nome de dispositivo reservado"
    );
    Ok(key)
}

pub fn valid_alias(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value
            .bytes()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-' || c == b'_')
}
impl Config {
    pub fn channel(&self, alias: &str, channel: &str) -> Result<ChannelId> {
        self.guild(alias)?.channel(channel)
    }
    pub fn guild(&self, alias: &str) -> Result<&GuildConfig> {
        self.guilds
            .values()
            .find(|g| g.alias.as_deref() == Some(alias))
            .with_context(|| format!("Servidor inexistente: {alias}"))
    }
}
impl GuildConfig {
    pub fn channel(&self, alias: &str) -> Result<ChannelId> {
        let id = match alias {
            "announcements" => self.channels.announcements,
            "rules" => self.channels.rules,
            "links" => self.channels.links,
            "moderation_logs" => self.channels.moderation_logs,
            other => self.channels.custom.get(other).copied(),
        };
        id.with_context(|| {
            format!(
                "Canal {alias} não configurado no servidor {}",
                self.guild_id
            )
        })
    }
    pub fn validate_registry(&self) -> Result<()> {
        if let Some(alias) = &self.alias {
            ensure!(valid_alias(alias), "Alias de servidor inválido");
        }
        for alias in self.channels.custom.keys() {
            ensure!(valid_alias(alias), "Alias de canal inválido");
            ensure!(
                !["announcements", "rules", "links", "moderation_logs"].contains(&alias.as_str()),
                "Alias de canal reservado/duplicado: {alias}"
            );
        }
        for (kind, path) in &self.templates {
            ensure!(
                ["announcements", "rules", "links"].contains(&kind.as_str()),
                "Tipo de template inválido"
            );
            ensure!(
                path.split('/').all(valid_alias) && path.split('/').count() == 2,
                "Referência de template inválida"
            );
        }
        Ok(())
    }
}
