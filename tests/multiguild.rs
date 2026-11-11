use discord_server_bot::config::{self, registry::folder_key};
use std::fs;
const BASE: &str = "app_name='Test'\nlog_level='info'\n[modules]\nadministration=true\nmessaging=false\nroles=true\n";

#[test]
fn rules_restart_override_reset_and_publish_preserve_guild_references() {
    use discord_server_bot::services::{
        content::Document,
        storage::{self, Publication, State, Store},
    };
    use poise::serenity_prelude as s;
    let dir = fixture(TWO);
    write(
        &dir,
        "guilds/defaults/rules.toml",
        "title='Default v1'\ndescription='Inherited'",
    );
    write(&dir, "guilds/O Acordo/rules.toml", "title='Guild v1'");
    let initial = config::load(dir.path()).unwrap();
    let one = initial.guild("one").unwrap();
    let two = initial.guild("two").unwrap();
    let mut state = State::default();
    let doc = storage::template(dir.path(), one, "rules", &state).unwrap();
    assert_eq!(doc.title, "Guild v1");
    assert_eq!(doc.description, "Inherited");
    let source = storage::content_source(one, "rules", &state);
    assert!(source.contains("defaults") && source.contains("O Acordo"));
    storage::remember_publication_content(&mut state, "rules", &doc, &doc);
    assert!(!state.documents.contains_key("rules")); // Plain publish must not freeze TOML.
    state.publications.insert(
        "rules".into(),
        Publication {
            channel: s::ChannelId::new(10),
            message: s::MessageId::new(20),
        },
    );
    let store = Store::open(dir.path().join("state")).unwrap();
    store.write(one.guild_id, &state).unwrap();
    store.write(two.guild_id, &state).unwrap();
    let other = fs::read(dir.path().join("state/guilds/2/state.json")).unwrap();

    write(
        &dir,
        "guilds/defaults/rules.toml",
        "title='Default v2'\ndescription='Inherited v2'",
    );
    write(&dir, "guilds/O Acordo/rules.toml", "title='Guild v2'");
    assert_eq!(
        storage::template(dir.path(), one, "rules", &state)
            .unwrap()
            .title,
        "Guild v1"
    );
    let restarted = config::load(dir.path()).unwrap();
    let one = restarted.guild("one").unwrap();
    let baseline = storage::template(dir.path(), one, "rules", &state).unwrap();
    assert_eq!(baseline.title, "Guild v2");
    assert_eq!(baseline.description, "Inherited v2");
    assert_eq!(
        restarted.guild("two").unwrap().contents["rules"].title,
        "Default v2"
    );
    let mut edited = baseline.clone();
    edited.title = "Edited in Discord".into();
    storage::remember_publication_content(&mut state, "rules", &edited, &baseline);
    assert!(!state.documents.contains_key("rules"));
    state.documents.insert("rules".into(), edited.clone()); // Legacy state from older versions.
    state.documents.insert(
        "links".into(),
        Document {
            title: "Links".into(),
            ..Default::default()
        },
    );
    state.profile.nickname = Some("Bot".into());
    store.write(one.guild_id, &state).unwrap();
    let mut state = store.read(one.guild_id).unwrap();
    assert!(!storage::content_source(one, "rules", &state).contains("JSON"));
    assert_eq!(
        storage::template(dir.path(), one, "rules", &state)
            .unwrap()
            .title,
        baseline.title
    );
    state.pending = Some("rules:operation".into());
    assert!(state.reset_rules().is_err());
    assert!(state.documents.contains_key("rules"));
    state.pending = None;
    assert!(state.reset_rules().unwrap());
    assert!(!state.reset_rules().unwrap());
    assert_eq!(state.profile.nickname.as_deref(), Some("Bot"));
    assert_eq!(state.documents["links"].title, "Links");
    assert_eq!(state.publications["rules"].channel.get(), 10);
    assert_eq!(state.publications["rules"].message.get(), 20);
    assert_eq!(
        storage::template(dir.path(), one, "rules", &state)
            .unwrap()
            .title,
        "Guild v2"
    );
    store.write(one.guild_id, &state).unwrap();
    assert_eq!(
        fs::read(dir.path().join("state/guilds/2/state.json")).unwrap(),
        other
    );
}
fn fixture(registry: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir_all(dir.path().join("guilds/defaults")).unwrap();
    fs::write(
        dir.path().join("default.toml"),
        format!("{BASE}\n{registry}"),
    )
    .unwrap();
    dir
}
fn write(dir: &tempfile::TempDir, path: &str, text: &str) {
    let path = dir.path().join(path);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}
const TWO: &str =
    "[guilds.one]\nid='1'\nname='O Acordo'\n[guilds.two]\nid='2'\nname='Comunidade Café'\n";
#[test]
fn folder_names_are_portable_and_unicode_aware() {
    for invalid in [
        "", " ", "defaults", "DEFAULTS", ".", "..", "../x", "a/b", "a\\b", "C:x", "a:", "CON",
        "con.txt", "LPT1", "COM¹", "x.", "x ", "/tmp", "a?b", "a\n",
    ] {
        assert!(folder_key(invalid).is_err(), "{invalid}");
    }
    assert_eq!(
        folder_key("Café").unwrap(),
        folder_key("CAFE\u{301}").unwrap()
    );
    assert_eq!(
        folder_key("Straße").unwrap(),
        folder_key("STRASSE").unwrap()
    );
    assert!(folder_key("O Acordo 日本語").is_ok());
}
#[test]
fn independent_defaults_commands_and_cached_content() {
    let dir = fixture(TWO);
    write(
        &dir,
        "guilds/defaults/config.toml",
        "[modules]\nrules=true\n[channels]\nrules='10'\n[channels.custom]\nevents='11'\n[roles]\nmoderators=['99']",
    );
    write(
        &dir,
        "guilds/defaults/rules.toml",
        "title='Shared'\ndescription='Intro'\n[[entries]]\nid='rule'\ntitle='Rule'\ndescription='Text'",
    );
    write(
        &dir,
        "guilds/O Acordo/config.toml",
        "[channels]\nrules='20'\n[roles]\nmoderators=[]",
    );
    write(
        &dir,
        "guilds/O Acordo/rules.toml",
        "title='Custom'\nentries=[]",
    );
    write(
        &dir,
        "guilds/O Acordo/commands.toml",
        "[commands]\n'rules.publish'=false\n'rules.update'=false",
    );
    let config = config::load(dir.path()).unwrap();
    let one = config.guild("one").unwrap();
    let two = config.guild("two").unwrap();
    assert_eq!(config.channel("one", "rules").unwrap().get(), 20);
    assert_eq!(two.channel("rules").unwrap().get(), 10);
    assert_eq!(one.channel("events").unwrap().get(), 11);
    assert!(one.roles.moderators.is_empty());
    assert_eq!(two.roles.moderators.len(), 1);
    assert_eq!(one.contents["rules"].description, "Intro");
    assert!(one.contents["rules"].entries.is_empty());
    assert_eq!(two.contents["rules"].entries.len(), 1);
    assert!(!one.command_enabled("rules.publish", config.global.modules));
    assert!(two.command_enabled("rules.publish", config.global.modules));
    let registered = config::availability::registered(one, config.global.modules);
    let rules = registered.iter().find(|c| c.name == "rules").unwrap();
    assert!(!rules.subcommands.iter().any(|c| c.name == "publish"));
    write(&dir, "guilds/O Acordo/rules.toml", "broken");
    let state = Default::default();
    let doc =
        discord_server_bot::services::storage::template(dir.path(), one, "rules", &state).unwrap();
    assert_eq!(doc.title, "Custom"); // No TOML reread after startup.
}
#[test]
fn invalid_registry_and_commands_are_rejected() {
    for registry in [
        TWO.replace("id='2'", "id='1'"),
        TWO.replace("Comunidade Café", "o acordo"),
        TWO.replace("id='2'", "id='0'"),
        TWO.replace("Comunidade Café", "defaults"),
        TWO.replace("guilds.two", "guilds.one"),
    ] {
        assert!(config::load(fixture(&registry).path()).is_err());
    }
    let dir = fixture(TWO);
    write(
        &dir,
        "guilds/defaults/commands.toml",
        "[commands]\nexec=true",
    );
    assert!(config::load(dir.path()).is_err());
}
#[test]
fn disabled_guild_and_modules_remove_commands() {
    let dir = fixture("[guilds.one]\nid='1'\nname='Disabled'\nenabled=false");
    let config = config::load(dir.path()).unwrap();
    assert!(
        config::availability::registered(config.guild("one").unwrap(), config.global.modules)
            .is_empty()
    );
    let dir = fixture(TWO);
    let config = config::load(dir.path()).unwrap();
    let registered =
        config::availability::registered(config.guild("one").unwrap(), config.global.modules);
    assert!(
        !registered
            .iter()
            .any(|c| c.name == "rules" || c.name == "announcements")
    );
}

#[test]
fn command_defaults_merge_and_empty_groups_disappear() {
    let dir = fixture(TWO);
    write(
        &dir,
        "guilds/defaults/commands.toml",
        "[commands]\nstatus=false\n'role.add'=false\n'role.remove'=false",
    );
    write(
        &dir,
        "guilds/O Acordo/commands.toml",
        "[commands]\nstatus=true",
    );
    let config = config::load(dir.path()).unwrap();
    let one = config.guild("one").unwrap();
    let two = config.guild("two").unwrap();
    assert!(one.command_enabled("ping.status", config.global.modules));
    assert!(!two.command_enabled("ping.status", config.global.modules));
    let commands = config::availability::registered(one, config.global.modules);
    assert!(!commands.iter().any(|c| c.name == "role"));
    let payload = poise::builtins::create_application_commands(&commands);
    let json = serde_json::to_value(payload).unwrap();
    assert!(json.as_array().unwrap().iter().any(|c| c["name"] == "ping"));
    write(&dir, "guilds/O Acordo/config.toml", "guild_id='99'");
    assert!(config::load(dir.path()).is_err());
}

#[test]
fn renaming_folder_keeps_persisted_content_by_id() {
    use discord_server_bot::services::{
        content::Document,
        storage::{State, template},
    };
    let dir = fixture(TWO);
    let mut state = State::default();
    state.documents.insert(
        "rules".into(),
        Document {
            title: "Persisted".into(),
            ..Default::default()
        },
    );
    let config = config::load(dir.path()).unwrap();
    let id = config.guild("one").unwrap().guild_id;
    fs::write(
        dir.path().join("default.toml"),
        format!("{BASE}\n{}", TWO.replace("O Acordo", "Novo Nome")),
    )
    .unwrap();
    let config = config::load(dir.path()).unwrap();
    assert_eq!(config.guild("one").unwrap().guild_id, id);
    assert_eq!(
        template(dir.path(), config.guild("one").unwrap(), "rules", &state)
            .unwrap()
            .title,
        config.guild("one").unwrap().contents["rules"].title
    );
}

#[test]
fn migration_refuses_existing_destination_without_changes() {
    let dir = fixture("");
    write(&dir, "guilds/old.toml", "guild_id='1'\nname='Existing'");
    write(&dir, "guilds/Existing/config.toml", "private");
    let original = fs::read(dir.path().join("default.toml")).unwrap();
    assert!(config::migration::migrate(dir.path(), dir.path()).is_err());
    assert_eq!(fs::read(dir.path().join("default.toml")).unwrap(), original);
    assert_eq!(
        fs::read_to_string(dir.path().join("guilds/Existing/config.toml")).unwrap(),
        "private"
    );
}
#[test]
fn migration_preserves_sources_templates_and_state() {
    let dir = fixture("");
    let legacy = "guild_id='1'\nalias='one'\nname='Café Guild'\n[channels]\nannouncements='9'\n";
    write(&dir, "guilds/old.toml", legacy);
    write(
        &dir,
        "old-templates/global/rules/default.toml",
        "title='Old rules'\ndescription='Preserved'",
    );
    write(&dir, "data/guilds/1/state.json", "private-state-marker");
    assert_eq!(
        config::migration::migrate(dir.path(), &dir.path().join("old-templates")).unwrap(),
        1
    );
    assert_eq!(
        fs::read_to_string(dir.path().join("guilds/old.toml")).unwrap(),
        legacy
    );
    assert_eq!(
        fs::read_to_string(dir.path().join("data/guilds/1/state.json")).unwrap(),
        "private-state-marker"
    );
    let loaded = config::load(dir.path()).unwrap();
    assert_eq!(loaded.guilds.len(), 1);
    assert_eq!(
        loaded.guild("one").unwrap().contents["rules"].title,
        "Old rules"
    );
    assert!(dir.path().join("default.toml.pre-multiguild").exists());
    assert!(config::migration::migrate(dir.path(), &dir.path().join("old-templates")).is_err());
}
