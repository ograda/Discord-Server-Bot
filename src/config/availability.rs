use super::*;
use anyhow::Context as _;

pub const ROOTS: &[&str] = &[
    "ping",
    "role",
    "role-all",
    "rules",
    "links",
    "announcement",
    "announce",
    "profile",
];
pub const COMMANDS: &[&str] = &[
    "ping",
    "ping.simples",
    "ping.status",
    "role",
    "role.add",
    "role.remove",
    "role-all",
    "role-all.add",
    "role-all.remove",
    "rules",
    "rules.preview",
    "rules.publish",
    "rules.update",
    "rules.reset",
    "rules.remove",
    "links",
    "links.preview",
    "links.publish",
    "links.update",
    "links.reset",
    "links.add",
    "links.remove",
    "announcement",
    "announcement.create",
    "announcement.edit",
    "announcement.remove",
    "announce",
    "profile",
    "profile.revert",
    "profile.nickname",
    "profile.bio",
    "profile.avatar",
];
pub fn default_action(root: &str) -> Option<&'static str> {
    match root {
        "ping" => Some("simples"),
        "rules" => Some("preview"),
        "announcement" => Some("create"),
        "profile" => Some("revert"),
        _ => None,
    }
}

impl GuildConfig {
    pub fn command_enabled(&self, path: &str, defaults: Modules) -> bool {
        if !self.enabled || !COMMANDS.contains(&path) {
            return false;
        }
        let modules = self.modules.unwrap_or(defaults);
        let (root, action) = path
            .split_once('.')
            .map_or((path, None), |(r, a)| (r, Some(a)));
        let active = match root {
            "ping" => true,
            "role" | "role-all" => modules.administration && modules.roles,
            "rules" => modules.administration && modules.rules,
            "links" => modules.administration && modules.links,
            "announce" | "announcement" => {
                modules.administration && modules.messaging && self.messaging.enabled
            }
            "profile" => modules.administration && modules.profile,
            _ => false,
        };
        active
            && self
                .commands
                .get(root)
                .copied()
                .unwrap_or(root != "role-all")
            && action.is_none_or(|action| {
                self.actions
                    .get(root)
                    .and_then(|a| a.get(action))
                    .copied()
                    .or_else(|| self.commands.get(path).copied())
                    .unwrap_or(true)
            })
    }
    pub fn select_action(
        &self,
        root: &str,
        requested: Option<&str>,
        defaults: Modules,
    ) -> Result<String> {
        let action = requested
            .or_else(|| default_action(root))
            .context("Selecione uma ação explicitamente")?;
        let path = format!("{root}.{action}");
        ensure!(
            self.command_enabled(&path, defaults),
            "Ação {path} desabilitada nesta guilda. Selecione explicitamente uma ação permitida."
        );
        Ok(action.into())
    }
}

/// Normalize each layer before overlay: a local legacy flag still overrides shared defaults.
pub fn apply_commands(guild: &mut GuildConfig, value: toml::Value) -> Result<()> {
    #[derive(Default, Deserialize)]
    #[serde(default, deny_unknown_fields)]
    struct Flags {
        commands: HashMap<String, bool>,
        actions: HashMap<String, HashMap<String, bool>>,
    }
    let flags: Flags = value.try_into()?;
    for (key, enabled) in flags.commands {
        let mapped = match key.as_str() {
            "status" => "ping.status".to_string(),
            "announcements" => "announcement".into(),
            "announcements.create" => "announcement.create".into(),
            "announcements.edit" => "announcement.edit".into(),
            "announcements.delete" => "announcement.remove".into(),
            "profile.apply" | "profile.show" | "announcements.list" | "rules.edit"
            | "rules.add" | "rules.list" | "links.edit" | "links.list" => {
                tracing::info!(guild = %guild.guild_id, command = key, "Entrada legada de comando descontinuada; arquivo preservado");
                continue;
            }
            _ => key.clone(),
        };
        ensure!(
            COMMANDS.contains(&mapped.as_str()),
            "Comando desconhecido: {key}"
        );
        if let Some((root, action)) = mapped.split_once('.') {
            guild
                .actions
                .entry(root.into())
                .or_default()
                .insert(action.into(), enabled);
        } else {
            guild.commands.insert(mapped, enabled);
        }
    }
    for (root, actions) in flags.actions {
        for (action, enabled) in actions {
            ensure!(
                COMMANDS.contains(&format!("{root}.{action}").as_str()),
                "Ação desconhecida: {root}.{action}"
            );
            guild
                .actions
                .entry(root.clone())
                .or_default()
                .insert(action, enabled);
        }
    }
    Ok(())
}
pub fn omission_reason(guild: &GuildConfig, path: &str, defaults: Modules) -> Option<&'static str> {
    if !guild.enabled {
        return Some("guilda desabilitada");
    }
    if !COMMANDS.contains(&path) {
        return Some("comando desconhecido");
    }
    if guild.command_enabled(path, defaults) {
        return None;
    }
    if guild.commands.get(path) == Some(&false)
        || path
            .split_once('.')
            .is_some_and(|(r, a)| guild.actions.get(r).and_then(|v| v.get(a)) == Some(&false))
    {
        return Some("commands.toml desabilita a entrada");
    }
    Some("módulo ou comando raiz desabilitado")
}
pub fn registered(
    guild: &GuildConfig,
    defaults: Modules,
) -> Vec<poise::Command<crate::core::Data, crate::core::Error>> {
    let mut commands = crate::core::commands::all();
    commands.retain_mut(|command| {
        if !guild.command_enabled(&command.name, defaults) {
            return false;
        }
        if command.name == "announce" {
            return true;
        }
        COMMANDS.iter().any(|p| {
            p.starts_with(&format!("{}.", command.name)) && guild.command_enabled(p, defaults)
        })
    });
    commands
}

/// Poise uses the original choice index as the wire value. Filtering its choice vector
/// would renumber actions. Rebuild only the remote option, preserving original indices.
pub fn remote_commands(
    guild: &GuildConfig,
    defaults: Modules,
) -> Vec<poise::serenity_prelude::CreateCommand> {
    registered(guild, defaults)
        .into_iter()
        .filter_map(|mut command| {
            let mut builder = command.create_as_slash_command()?;
            if command.subcommands.is_empty() {
                let options = command
                    .parameters
                    .iter_mut()
                    .filter_map(|p| {
                        if p.name != "ação" && p.name != "modo" {
                            return p.create_as_slash_command_option();
                        }
                        if command.name == "links" && p.name == "ação" {
                            let mut option = poise::serenity_prelude::CreateCommandOption::new(
                                poise::serenity_prelude::CommandOptionType::String,
                                &p.name,
                                "Operação obrigatória do catálogo de Links",
                            )
                            .required(true);
                            for choice in &p.choices {
                                if guild
                                    .command_enabled(&format!("links.{}", choice.name), defaults)
                                {
                                    option = option.add_string_choice(&choice.name, &choice.name);
                                }
                            }
                            return Some(option);
                        }
                        let choices = std::mem::take(&mut p.choices);
                        let mut option = p.create_as_slash_command_option()?;
                        for (index, choice) in choices.iter().enumerate() {
                            if guild.command_enabled(
                                &format!("{}.{}", command.name, choice.name),
                                defaults,
                            ) {
                                option = option.add_int_choice(&choice.name, index as i32);
                            }
                        }
                        Some(option)
                    })
                    .collect();
                builder = builder.set_options(options);
            }
            Some(builder)
        })
        .collect()
}
