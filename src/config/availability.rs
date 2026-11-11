use super::*;
pub const COMMANDS: &[&str] = &[
    "ping",
    "status",
    "announce",
    "role.add",
    "role.remove",
    "announcements.create",
    "announcements.edit",
    "announcements.list",
    "announcements.delete",
    "rules.preview",
    "rules.publish",
    "rules.update",
    "rules.edit",
    "rules.add",
    "rules.remove",
    "rules.list",
    "links.preview",
    "links.publish",
    "links.update",
    "links.edit",
    "links.add",
    "links.remove",
    "links.list",
];

impl GuildConfig {
    pub fn command_enabled(&self, path: &str, defaults: Modules) -> bool {
        let modules = self.modules.unwrap_or(defaults);
        if !self.enabled {
            return false;
        }
        let module = path.split('.').next().unwrap_or(path);
        let active = match module {
            "ping" | "status" => true,
            "role" => modules.administration && modules.roles,
            "announce" | "announcements" => {
                modules.administration && modules.messaging && self.messaging.enabled
            }
            "rules" => modules.administration && modules.rules,
            "links" => modules.administration && modules.links,
            _ => false,
        };
        active && COMMANDS.contains(&path) && self.commands.get(path).copied().unwrap_or(true)
    }
}
pub fn registered(
    guild: &GuildConfig,
    defaults: Modules,
) -> Vec<poise::Command<crate::core::Data, crate::core::Error>> {
    let mut commands = crate::core::commands::all();
    commands.retain_mut(|command| {
        if command.subcommands.is_empty() {
            return guild.command_enabled(&command.name, defaults);
        }
        command.subcommands.retain(|child| {
            guild.command_enabled(&format!("{}.{}", command.name, child.name), defaults)
        });
        !command.subcommands.is_empty()
    });
    commands
}
