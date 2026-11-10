use discord_server_bot::config::{self, GlobalConfig, GuildConfig};
use poise::serenity_prelude::GuildId;
use std::fs;

const GLOBAL: &str = include_str!("../config/default.toml");
const GUILD: &str = include_str!("../config/guilds/example.toml.example");

fn fixture() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("default.toml"), GLOBAL).unwrap();
    fs::create_dir(dir.path().join("guilds")).unwrap();
    fs::write(dir.path().join("guilds/one.toml"), GUILD).unwrap();
    dir
}
#[test]
fn loads_and_selects_exact_id_ignoring_examples() {
    let dir = fixture();
    fs::write(dir.path().join("guilds/broken.toml.example"), "invalid").unwrap();
    let config = config::load(dir.path()).unwrap();
    assert_eq!(config.guilds.len(), 1);
    assert!(
        config
            .guilds
            .contains_key(&GuildId::new(123456789012345678))
    );
    assert!(!config.guilds.contains_key(&GuildId::new(42)));
}
#[test]
fn rejects_duplicate_servers() {
    let dir = fixture();
    fs::write(dir.path().join("guilds/two.toml"), GUILD).unwrap();
    assert!(
        config::load(dir.path())
            .unwrap_err()
            .to_string()
            .contains("duplicado")
    );
}
#[test]
fn invalid_file_includes_path() {
    let dir = fixture();
    fs::write(dir.path().join("guilds/broken.toml"), "[broken").unwrap();
    assert!(
        config::load(dir.path())
            .unwrap_err()
            .to_string()
            .contains("broken.toml")
    );
}
#[test]
fn rejects_empty_allowlist() {
    let dir = fixture();
    fs::remove_file(dir.path().join("guilds/one.toml")).unwrap();
    assert!(config::load(dir.path()).is_err());
}
#[test]
fn validates_global_fields() {
    for text in [
        GLOBAL.replace("Discord Server Bot", " "),
        GLOBAL.replace("info", "invalid"),
    ] {
        assert!(
            toml::from_str::<GlobalConfig>(&text)
                .unwrap()
                .validate()
                .is_err()
        );
    }
    assert!(toml::from_str::<GlobalConfig>("app_name = 'test'").is_err());
}
#[test]
fn validates_ids_and_unknown_fields() {
    for id in ["0", "-1", "abc", "18446744073709551616"] {
        assert!(
            toml::from_str::<GuildConfig>(&GUILD.replace("123456789012345678", id)).is_err(),
            "{id}"
        );
    }
    assert!(toml::from_str::<GuildConfig>(&format!("unknown = true\n{GUILD}")).is_err());
    assert!(toml::from_str::<GuildConfig>("[modules]").is_err());
}
#[test]
fn validates_announcement_channel_and_duplicate_roles() {
    let global: GlobalConfig = toml::from_str(GLOBAL).unwrap();
    let text = GUILD.replace("announcements = \"123456789012345679\"", "");
    assert!(
        toml::from_str::<GuildConfig>(&text)
            .unwrap()
            .validate(global.modules)
            .is_err()
    );
    let text = GUILD.replace(
        "[\"123456789012345681\"]",
        "[\"123456789012345681\", \"123456789012345681\"]",
    );
    assert!(
        toml::from_str::<GuildConfig>(&text)
            .unwrap()
            .validate(global.modules)
            .is_err()
    );
}
#[test]
fn inherits_module_defaults() {
    let global: GlobalConfig = toml::from_str(GLOBAL).unwrap();
    let guild: GuildConfig =
        toml::from_str("guild_id = '123'\n[channels]\nannouncements = '456'").unwrap();
    assert!(guild.modules.is_none());
    guild.validate(global.modules).unwrap();
}
