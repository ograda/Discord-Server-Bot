use discord_server_bot::{
    config::{self, availability},
    core::{commands, permissions},
    services::{
        announcements,
        catalog::Catalog,
        content::{Document, Entry},
        profile,
        publishing::{self, Plan},
        storage::{Publication, State, Store},
    },
};
use poise::serenity_prelude as s;
use std::{
    fs,
    io::{Cursor, Read, Write},
    path::Path,
};

const GLOBAL: &str = "app_name='Test'\nlog_level='info'\n[modules]\nadministration=true\nmessaging=true\nroles=true\nrules=true\nlinks=true\nprofile=true\n[guilds.one]\nid='1'\nname='One'\n[guilds.two]\nid='2'\nname='Two'";
const LINKS: &str = "# Shared boards\n[[boards]]\nid='important'\ntitle='Links importantes'\nchannel='links'\n[[boards.links]]\nid='site'\ntitle='Site'\nurl='https://example.org'\n[[boards]]\nid='guides'\ntitle='Guias'\nchannel='resources'\n";

#[test]
fn public_toml_examples_have_valid_syntax() {
    for source in [
        include_str!("../config/guilds/commands.toml.example"),
        include_str!("../config/guilds/rules.toml.example"),
        include_str!("../config/guilds/links.toml.example"),
        include_str!("../config/guilds/announcements.toml.example"),
        include_str!("../config/guilds/defaults/commands.toml"),
    ] {
        toml::from_str::<toml::Value>(source).unwrap();
    }
}
fn write(root: &Path, path: &str, content: &str) {
    let path = root.join(path);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, content).unwrap();
}
fn fixture() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "default.toml", GLOBAL);
    write(
        dir.path(),
        "guilds/defaults/config.toml",
        "[channels]\nannouncements='10'\nrules='11'\nlinks='12'\n[channels.custom]\nresources='13'\n[profile]\nnickname='TOML'\nbio='Default bio'",
    );
    write(
        dir.path(),
        "guilds/defaults/rules.toml",
        "# shared\ntitle='Rules v1'\ndescription='Shared description'",
    );
    write(dir.path(), "guilds/defaults/links.toml", LINKS);
    write(
        dir.path(),
        "guilds/defaults/announcements.toml",
        "title='Aviso'\ncolor=5793266\nfooter='Equipe'\nthumbnail='https://example.org/logo.png'",
    );
    dir
}

#[test]
fn roots_native_roles_optional_choices_and_attachment_schema() {
    let all = commands::all();
    assert_eq!(
        all.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(),
        availability::ROOTS
    );
    let payload = serde_json::to_value(poise::builtins::create_application_commands(&all)).unwrap();
    for command in payload.as_array().unwrap() {
        let name = command["name"].as_str().unwrap();
        let options = command["options"].as_array().unwrap();
        if name == "role" || name == "role-all" {
            assert_eq!(options.len(), if name == "role" { 3 } else { 2 });
            assert!(
                options
                    .iter()
                    .all(|o| o["required"] == true && o["type"] != 1 && o["type"] != 3)
            );
            assert_eq!(availability::default_action(name), None);
        } else {
            assert!(options.iter().all(|o| o["type"] != 1 && o["type"] != 2));
            if name != "announce" {
                let choice = options
                    .iter()
                    .find(|o| o["name"] == if name == "ping" { "modo" } else { "ação" })
                    .unwrap();
                assert_eq!(choice["required"], name == "links");
                assert!(!choice["choices"].as_array().unwrap().is_empty());
                assert_eq!(choice["autocomplete"], false);
            }
        }
    }
    let profile = payload
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == "profile")
        .unwrap();
    let image = profile["options"]
        .as_array()
        .unwrap()
        .iter()
        .find(|o| o["name"] == "imagem")
        .unwrap();
    assert_eq!(image["type"], 11);
    assert_eq!(image["required"], false);
    let announce = all.iter().find(|c| c.name == "announce").unwrap();
    assert!(
        announce
            .parameters
            .iter()
            .find(|p| p.name == "mensagem")
            .unwrap()
            .required
    );
    assert!(
        !announce
            .parameters
            .iter()
            .find(|p| p.name == "canal")
            .unwrap()
            .required
    );
    for (root, default) in [
        ("rules", "preview"),
        ("announcement", "create"),
        ("profile", "revert"),
        ("ping", "simples"),
    ] {
        assert_eq!(availability::default_action(root), Some(default));
    }
}

#[test]
fn serenity_resolves_avatar_attachment_without_rejecting_1254_dimensions() {
    let data: s::CommandData = serde_json::from_value(serde_json::json!({
        "id":"1", "name":"profile", "type":1,
        "resolved":{"attachments":{"42":{
            "id":"42", "filename":"avatar.png", "size":1024,
            "url":"https://cdn.discordapp.com/attachments/1/42/avatar.png",
            "proxy_url":"https://media.discordapp.net/attachments/1/42/avatar.png",
            "width":1254, "height":1254, "content_type":"image/png"
        }}},
        "options":[{"name":"ação","type":4,"value":3},{"name":"imagem","type":11,"value":"42"}]
    }))
    .unwrap();
    let options = data.options();
    let option = options.iter().find(|v| v.name == "imagem").unwrap();
    match option.value {
        s::ResolvedValue::Attachment(attachment) => {
            assert_eq!(attachment.dimensions(), Some((1254, 1254)));
            assert_eq!(attachment.content_type.as_deref(), Some("image/png"));
            assert_eq!(attachment.id.get(), 42);
        }
        _ => panic!("Attachment must resolve through Serenity"),
    }
}

#[test]
fn guild_filters_preserve_choice_values_and_disabled_defaults_require_explicit_action() {
    let dir = fixture();
    write(
        dir.path(),
        "guilds/defaults/commands.toml",
        "[commands]\nping=true\n[actions.rules]\npreview=false\npublish=false\n[actions.role]\nremove=false",
    );
    write(
        dir.path(),
        "guilds/One/commands.toml",
        "[commands]\n'profile.show'=false\n'announcements.delete'=false\n[actions.rules]\npublish=true",
    );
    let config = config::load(dir.path()).unwrap();
    let one = config.guild("one").unwrap();
    let two = config.guild("two").unwrap();
    assert!(
        one.select_action("rules", None, config.global.modules)
            .is_err()
    );
    assert_eq!(
        one.select_action("rules", Some("publish"), config.global.modules)
            .unwrap(),
        "publish"
    );
    assert!(
        two.select_action("rules", Some("publish"), config.global.modules)
            .is_err()
    );
    assert!(!one.command_enabled("announcement.remove", config.global.modules));
    let remote =
        serde_json::to_value(availability::remote_commands(one, config.global.modules)).unwrap();
    let rules = remote
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == "rules")
        .unwrap();
    let choices = rules["options"][0]["choices"].as_array().unwrap();
    assert!(!choices.iter().any(|c| c["name"] == "preview"));
    assert_eq!(
        choices.iter().find(|c| c["name"] == "publish").unwrap()["value"],
        1
    );
    assert_eq!(
        choices.iter().find(|c| c["name"] == "update").unwrap()["value"],
        2
    );
    let role = remote
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == "role")
        .unwrap();
    assert_eq!(role["options"].as_array().unwrap().len(), 3);
    assert_eq!(role["options"][0]["choices"].as_array().unwrap().len(), 1);
    write(
        dir.path(),
        "guilds/One/commands.toml",
        "[commands]\nrules=false\n[actions.links]\npreview=false\npublish=false\nupdate=false\nreset=false\nadd=false\nremove=false",
    );
    let config = config::load(dir.path()).unwrap();
    assert!(
        !availability::registered(config.guild("one").unwrap(), config.global.modules)
            .iter()
            .any(|c| c.name == "rules" || c.name == "links")
    );
    write(
        dir.path(),
        "guilds/One/config.toml",
        "[modules]\nprofile=false",
    );
    let config = config::load(dir.path()).unwrap();
    assert!(
        !config
            .guild("one")
            .unwrap()
            .command_enabled("profile.avatar", config.global.modules)
    );
}

#[test]
fn conditional_parameters_reject_cross_action_arguments() {
    for (root, action, parameters) in [
        ("links", "add", vec!["id"]),
        ("links", "publish", vec!["id"]),
        ("links", "remove", vec!["id"]),
        ("announcement", "edit", vec!["mensagem", "identificador"]),
        ("profile", "avatar", vec!["imagem", "normalizar"]),
        ("profile", "nickname", vec!["apelido"]),
        ("profile", "revert", vec![]),
        ("rules", "update", vec![]),
    ] {
        assert!(permissions::conditional_parameters(root, action, &parameters).is_ok());
    }
    for (root, action, parameter) in [
        ("links", "update", "quadro"),
        ("links", "preview", "url"),
        ("profile", "revert", "imagem"),
        ("profile", "bio", "apelido"),
        ("profile", "avatar", "biografia"),
        ("announcement", "create", "identificador"),
        ("announcement", "remove", "mensagem"),
    ] {
        assert!(permissions::conditional_parameters(root, action, &[parameter]).is_err());
    }
}

#[tokio::test]
async fn explicit_reload_is_atomic_uses_fallback_and_isolates_guilds() {
    let dir = fixture();
    write(dir.path(), "guilds/One/rules.toml", "title='One initial'");
    let config = config::load(dir.path()).unwrap();
    let catalog = Catalog::new(&config);
    let one = config.guild("one").unwrap();
    let two = config.guild("two").unwrap();
    write(dir.path(), "guilds/One/rules.toml", "title='One new'");
    assert_eq!(
        catalog.snapshot(one.guild_id).await.unwrap().rules.title,
        "One initial"
    );
    let source = catalog.reload(&config.root, one, "rules").await.unwrap();
    assert!(source.contains("One") && source.contains("rules.toml"));
    let snapshot = catalog.snapshot(one.guild_id).await.unwrap();
    assert_eq!(snapshot.rules.title, "One new");
    assert_eq!(snapshot.rules.description, "Shared description");
    write(dir.path(), "guilds/One/rules.toml", "invalid TOML");
    assert!(catalog.reload(&config.root, one, "rules").await.is_err());
    assert_eq!(
        catalog.snapshot(one.guild_id).await.unwrap().revision,
        snapshot.revision
    );
    assert_eq!(
        catalog.snapshot(one.guild_id).await.unwrap().rules,
        snapshot.rules
    );
    fs::remove_file(dir.path().join("guilds/One/rules.toml")).unwrap();
    let source = catalog.reload(&config.root, one, "rules").await.unwrap();
    assert!(source.contains("defaults") && !source.contains("One"));
    assert_eq!(
        catalog.snapshot(one.guild_id).await.unwrap().rules.title,
        "Rules v1"
    );
    assert_eq!(catalog.snapshot(two.guild_id).await.unwrap().revision, 0);
    write(
        dir.path(),
        "guilds/One/links.toml",
        "[[boards]]\nid='x'\ntitle='X'\n[[boards.links]]\nid='bad'\ntitle='Bad'\nurl='javascript:alert(1)'",
    );
    assert!(catalog.reload(&config.root, one, "links").await.is_err());
    assert_eq!(
        catalog
            .snapshot(one.guild_id)
            .await
            .unwrap()
            .links
            .boards
            .len(),
        2
    );
}

#[tokio::test]
async fn reset_preserves_exact_backup_and_does_not_write_json_or_other_guilds() {
    let dir = fixture();
    let config = config::load(dir.path()).unwrap();
    let catalog = Catalog::new(&config);
    let one = config.guild("one").unwrap();
    for kind in ["rules", "links"] {
        let original = format!("# private local edit for {kind}\ninvalid TOML");
        write(dir.path(), &format!("guilds/One/{kind}.toml"), &original);
        let (_, backup) = catalog.reset(&config.root, one, kind).await.unwrap();
        assert_eq!(fs::read_to_string(backup.unwrap()).unwrap(), original);
        assert_eq!(
            fs::read(dir.path().join(format!("guilds/One/{kind}.toml"))).unwrap(),
            fs::read(dir.path().join(format!("guilds/defaults/{kind}.toml"))).unwrap()
        );
    }
    let before = fs::read(dir.path().join("guilds/One/rules.toml")).unwrap();
    write(dir.path(), "guilds/defaults/rules.toml", "invalid");
    assert!(catalog.reset(&config.root, one, "rules").await.is_err());
    assert_eq!(
        fs::read(dir.path().join("guilds/One/rules.toml")).unwrap(),
        before
    );
    assert!(!dir.path().join("guilds/Two/rules.toml").exists());
    assert!(!dir.path().join("data").exists());
}

fn link(id: &str, url: &str) -> Entry {
    Entry {
        id: id.into(),
        title: "Convite da comunidade".into(),
        url: url.into(),
        ..Default::default()
    }
}
#[tokio::test]
async fn links_add_persists_comments_boards_optional_description_and_invites() {
    let dir = fixture();
    let local = LINKS.replace("# Shared boards", "# Preserve this local comment");
    write(dir.path(), "guilds/One/links.toml", &local);
    let config = config::load(dir.path()).unwrap();
    let one = config.guild("one").unwrap();
    let catalog = Catalog::new(&config);
    let (_, backup) = catalog
        .add_link(
            &config.root,
            one,
            "important",
            link("discord", "https://discord.gg/XnxQ9q9VB"),
        )
        .await
        .unwrap();
    assert_eq!(fs::read_to_string(backup.unwrap()).unwrap(), local);
    let bytes = fs::read(dir.path().join("guilds/One/links.toml")).unwrap();
    assert!(String::from_utf8_lossy(&bytes).contains("# Preserve this local comment"));
    let snapshot = catalog.snapshot(one.guild_id).await.unwrap();
    assert_eq!(snapshot.links.boards[0].links.len(), 2);
    assert_eq!(snapshot.links.boards[1].id, "guides");
    assert_eq!(snapshot.links.boards[1].channel, "resources");
    assert!(snapshot.links.boards[0].document().embeds("links").is_ok());
    assert_eq!(
        snapshot.links.boards[0].publication_key(),
        "links:important"
    );
    assert_eq!(
        catalog
            .snapshot(s::GuildId::new(2))
            .await
            .unwrap()
            .links
            .boards[0]
            .links
            .len(),
        1
    );
    for (board, entry) in [
        ("important", link("discord", "https://example.org")),
        ("important", link("invalid", "javascript:alert(1)")),
        ("missing", link("id", "http://example.org")),
    ] {
        assert!(
            catalog
                .add_link(&config.root, one, board, entry)
                .await
                .is_err()
        );
        assert_eq!(
            fs::read(dir.path().join("guilds/One/links.toml")).unwrap(),
            bytes
        );
    }
    let reloaded = config::load(dir.path()).unwrap();
    assert_eq!(
        reloaded.guild("one").unwrap().link_boards.as_ref().unwrap()[0]
            .links
            .len(),
        2
    );
}

#[tokio::test]
async fn links_legacy_partial_file_materializes_inherited_entries_without_losing_them() {
    let dir = fixture();
    write(
        dir.path(),
        "guilds/defaults/links.toml",
        "title='Legacy'\n[[entries]]\nid='old'\ntitle='Old'\nurl='http://example.org'",
    );
    write(
        dir.path(),
        "guilds/One/links.toml",
        "# keep\ntitle='Local legacy'",
    );
    let config = config::load(dir.path()).unwrap();
    let catalog = Catalog::new(&config);
    catalog
        .add_link(
            &config.root,
            config.guild("one").unwrap(),
            "default",
            link("new", "https://discord.gg/test"),
        )
        .await
        .unwrap();
    let reload = config::load(dir.path()).unwrap();
    let boards = reload.guild("one").unwrap().link_boards.as_ref().unwrap();
    assert_eq!(boards[0].links.len(), 2);
    assert_eq!(boards[0].links[0].id, "old");
    assert_eq!(boards[0].title, "Local legacy");
    assert_eq!(boards[0].publication_key(), "links");
    assert!(
        fs::read_to_string(dir.path().join("guilds/One/links.toml"))
            .unwrap()
            .contains("# keep")
    );
}

#[tokio::test]
async fn links_remove_and_board_edit_preserve_comments_other_boards_publications_and_defaults() {
    let dir = fixture();
    let config = config::load(dir.path()).unwrap();
    let one = config.guild("one").unwrap();
    let catalog = Catalog::new(&config);
    let store = Store::open(dir.path().join("data")).unwrap();
    let mut state = State::default();
    state.publications.insert(
        "links:important".into(),
        Publication {
            channel: s::ChannelId::new(12),
            message: s::MessageId::new(777),
        },
    );
    store.write(one.guild_id, &state).unwrap();
    let state_path = dir.path().join("data/guilds/1/state.json");
    let before_state = fs::read(&state_path).unwrap();
    let inherited = fs::read(dir.path().join("guilds/defaults/links.toml")).unwrap();
    let original = catalog.snapshot(one.guild_id).await.unwrap().links.boards[0].clone();
    let mut extra = link("new", "https://chat.whatsapp.com/EXEMPLO");
    extra.label = Some("Entrar no grupo".into());
    catalog
        .add_link(&config.root, one, "important", extra.clone())
        .await
        .unwrap();
    let target = dir.path().join("guilds/One/links.toml");
    let before = fs::read_to_string(&target).unwrap();
    assert!(before.contains("# Shared boards"));
    assert!(before.contains("label = \"Entrar no grupo\""));
    let (_, backup) = catalog
        .remove_link(&config.root, one, "important", original.links[0].clone())
        .await
        .unwrap();
    assert_eq!(fs::read(backup.unwrap()).unwrap(), before.as_bytes());
    let current = catalog.snapshot(one.guild_id).await.unwrap().links.boards[0].document();
    assert_eq!(current.entries, vec![extra.clone()]);
    let mut edited = current.clone();
    edited.title = "Edited title".into();
    catalog
        .edit_board(&config.root, one, "important", current, edited.clone())
        .await
        .unwrap();
    assert_eq!(
        catalog.snapshot(one.guild_id).await.unwrap().links.boards[0].document(),
        edited
    );
    assert!(
        fs::read_to_string(&target)
            .unwrap()
            .contains("# Shared boards")
    );
    catalog
        .remove_link(&config.root, one, "important", extra)
        .await
        .unwrap();
    let reloaded = config::load(dir.path()).unwrap();
    let boards = reloaded.guild("one").unwrap().link_boards.as_ref().unwrap();
    assert!(boards[0].links.is_empty());
    assert_eq!(boards[1].id, "guides");
    assert_eq!(fs::read(&state_path).unwrap(), before_state);
    assert_eq!(
        fs::read(dir.path().join("guilds/defaults/links.toml")).unwrap(),
        inherited
    );
    assert!(!dir.path().join("guilds/Two/links.toml").exists());
    assert_eq!(
        catalog
            .snapshot(s::GuildId::new(2))
            .await
            .unwrap()
            .links
            .boards[0],
        original
    );
}

#[tokio::test]
async fn links_remove_legacy_inheritance_materializes_empty_array_and_rejects_stale_selection() {
    let dir = fixture();
    write(
        dir.path(),
        "guilds/defaults/links.toml",
        "title='Legacy'\n[[entries]]\nid='site'\ntitle='Site'\nurl='https://example.org'\nlabel='Visitar'\n",
    );
    write(
        dir.path(),
        "guilds/One/links.toml",
        "# Local comment\ntitle='Local'",
    );
    let config = config::load(dir.path()).unwrap();
    let one = config.guild("one").unwrap();
    let catalog = Catalog::new(&config);
    let entry = catalog.snapshot(one.guild_id).await.unwrap().links.boards[0].links[0].clone();
    let mut stale = entry.clone();
    stale.title = "Old title".into();
    assert!(
        catalog
            .remove_link(&config.root, one, "default", stale)
            .await
            .is_err()
    );
    assert_eq!(
        fs::read_to_string(dir.path().join("guilds/One/links.toml")).unwrap(),
        "# Local comment\ntitle='Local'"
    );
    catalog
        .remove_link(&config.root, one, "default", entry)
        .await
        .unwrap();
    let content = fs::read_to_string(dir.path().join("guilds/One/links.toml")).unwrap();
    assert!(content.contains("# Local comment"));
    assert!(content.contains("entries = []"));
    let reloaded = config::load(dir.path()).unwrap();
    assert!(
        reloaded.guild("one").unwrap().link_boards.as_ref().unwrap()[0]
            .links
            .is_empty()
    );
    assert_eq!(
        reloaded.guild("two").unwrap().link_boards.as_ref().unwrap()[0]
            .links
            .len(),
        1
    );
}

#[tokio::test]
async fn empty_boards_remain_empty_after_restart_and_duplicate_ids_are_rejected() {
    let dir = fixture();
    write(dir.path(), "guilds/One/links.toml", "boards=[]");
    let config = config::load(dir.path()).unwrap();
    let catalog = Catalog::new(&config);
    assert!(
        catalog
            .snapshot(s::GuildId::new(1))
            .await
            .unwrap()
            .links
            .boards
            .is_empty()
    );
    write(
        dir.path(),
        "guilds/One/links.toml",
        "[[boards]]\nid='x'\n[[boards]]\nid='x'",
    );
    assert!(
        catalog
            .reload(&config.root, config.guild("one").unwrap(), "links")
            .await
            .is_err()
    );
    assert!(
        catalog
            .snapshot(s::GuildId::new(1))
            .await
            .unwrap()
            .links
            .boards
            .is_empty()
    );
}

#[test]
fn legacy_json_is_backed_up_and_archived_without_losing_message_ids() {
    let dir = fixture();
    let config = config::load(dir.path()).unwrap();
    let guild = s::GuildId::new(1);
    let store = Store::open(dir.path().join("data")).unwrap();
    let mut state = State::default();
    state.documents.insert(
        "rules".into(),
        Document {
            title: "Old rules".into(),
            ..Default::default()
        },
    );
    state.documents.insert(
        "links".into(),
        Document {
            title: "Old links".into(),
            ..Default::default()
        },
    );
    state.publications.insert(
        "links".into(),
        Publication {
            channel: s::ChannelId::new(12),
            message: s::MessageId::new(99),
        },
    );
    state.publications.insert(
        "rules".into(),
        Publication {
            channel: s::ChannelId::new(11),
            message: s::MessageId::new(98),
        },
    );
    state.profile.nickname = Some("Custom".into());
    store.write(guild, &state).unwrap();
    let path = dir.path().join("data/guilds/1");
    let original = fs::read(path.join("state.json")).unwrap();
    assert!(
        store
            .migrate_content(
                guild,
                config
                    .guild("one")
                    .unwrap()
                    .link_boards
                    .as_ref()
                    .unwrap()
                    .first()
            )
            .unwrap()
    );
    let migrated = store.read(guild).unwrap();
    assert_eq!(migrated.archived_documents.len(), 2);
    assert!(migrated.documents.is_empty());
    assert_eq!(migrated.publications["links:important"].message.get(), 99);
    assert_eq!(migrated.publications["rules"].message.get(), 98);
    assert_eq!(migrated.profile.nickname.as_deref(), Some("Custom"));
    let backups: Vec<_> = fs::read_dir(&path)
        .unwrap()
        .filter_map(|v| {
            let p = v.unwrap().path();
            p.to_string_lossy().ends_with(".json.bak").then_some(p)
        })
        .collect();
    assert_eq!(backups.len(), 1);
    assert_eq!(fs::read(&backups[0]).unwrap(), original);
    assert!(!store.migrate_content(guild, None).unwrap());
    assert_eq!(store.read(s::GuildId::new(2)).unwrap().revision, 0);
    assert_eq!(
        discord_server_bot::services::storage::template(
            Path::new("templates"),
            config.guild("one").unwrap(),
            "rules",
            &state
        )
        .unwrap()
        .title,
        "Rules v1"
    );
}

#[test]
fn profile_revert_restores_defaults_and_serializes_null_for_absent_values() {
    let dir = fixture();
    let config = config::load(dir.path()).unwrap();
    let one = config.guild("one").unwrap();
    let defaults = profile::load_defaults(&config.root, one).unwrap();
    let patch = profile::revert(&defaults);
    assert_eq!(
        serde_json::to_value(patch).unwrap(),
        serde_json::json!({"nick":"TOML","bio":"Default bio","avatar":null})
    );
    assert_eq!(
        serde_json::to_value(profile::revert(&profile::ProfileConfig::default())).unwrap(),
        serde_json::json!({"nick":null,"bio":null,"avatar":null})
    );
    let mut image = Cursor::new(Vec::new());
    image::DynamicImage::new_rgba8(512, 512)
        .write_to(&mut image, image::ImageFormat::Png)
        .unwrap();
    fs::create_dir_all(dir.path().join("guilds/defaults/assets")).unwrap();
    fs::write(
        dir.path().join("guilds/defaults/assets/avatar.png"),
        image.into_inner(),
    )
    .unwrap();
    write(
        dir.path(),
        "guilds/defaults/config.toml",
        "[channels]\nannouncements='10'\nrules='11'\nlinks='12'\n[channels.custom]\nresources='13'\n[profile]\nnickname='Shared'\navatar='assets/avatar.png'",
    );
    write(
        dir.path(),
        "guilds/One/config.toml",
        "[profile]\nnickname='Local'",
    );
    let reloaded = config::load(dir.path()).unwrap();
    let profile = profile::load_defaults(&reloaded.root, reloaded.guild("one").unwrap()).unwrap();
    assert_eq!(profile.nickname.as_deref(), Some("Local"));
    assert!(
        profile
            .avatar_data
            .unwrap()
            .starts_with("data:image/png;base64,")
    );
}

#[test]
fn announcement_selection_and_direct_messages_preserve_template_style() {
    let dir = fixture();
    let config = config::load(dir.path()).unwrap();
    let base = &config.guild("one").unwrap().contents["announcements"];
    let doc = announcements::with_message(base, "Aviso @everyone").unwrap();
    assert_eq!(doc.title, base.title);
    assert_eq!(doc.color, base.color);
    assert_eq!(doc.footer, base.footer);
    assert_eq!(doc.thumbnail, base.thumbnail);
    assert!(doc.text.is_empty());
    assert_eq!(doc.embeds("announcements").unwrap().len(), 1);
    assert!(announcements::with_message(base, " ").is_err());
    let mut state = State::default();
    state.documents.insert("announcement-123".into(), doc);
    state.publications.insert(
        "announcement-123".into(),
        Publication {
            channel: s::ChannelId::new(10),
            message: s::MessageId::new(20),
        },
    );
    assert_eq!(announcements::choices(&state, "Aviso").len(), 1);
    assert_eq!(
        announcements::resolve_in_guild(
            &state,
            s::GuildId::new(1),
            "https://discord.com/channels/1/10/20"
        )
        .unwrap(),
        "announcement-123"
    );
    for url in [
        "https://discord.com/channels/2/10/20",
        "https://discord.com/channels/1/99/20",
        "https://evil.test/channels/1/10/20",
    ] {
        assert!(announcements::resolve_in_guild(&state, s::GuildId::new(1), url).is_err());
    }
}

// A local HTTP stand-in exercises Serenity's real request builders without Discord credentials.
fn mock_discord(
    responses: Vec<(u16, String)>,
) -> (
    s::Http,
    std::thread::JoinHandle<Vec<(String, serde_json::Value)>>,
) {
    use std::time::{Duration, Instant};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let address = listener.local_addr().unwrap();
    let handle = std::thread::spawn(move || {
        let mut records = vec![];
        for (status, body) in responses {
            let deadline = Instant::now() + Duration::from_secs(10);
            let mut socket = loop {
                match listener.accept() {
                    Ok((socket, _)) => break socket,
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        assert!(Instant::now() < deadline, "expected HTTP request");
                        std::thread::sleep(Duration::from_millis(5));
                    }
                    Err(e) => panic!("{e}"),
                }
            };
            socket.set_nonblocking(false).unwrap();
            socket
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut request = Vec::new();
            let (headers, payload) = loop {
                let mut bytes = [0; 8192];
                let n = socket.read(&mut bytes).unwrap();
                assert!(n > 0);
                request.extend_from_slice(&bytes[..n]);
                if let Some(i) = request.windows(4).position(|w| w == b"\r\n\r\n") {
                    let header = String::from_utf8(request[..i].to_vec()).unwrap();
                    let len = header
                        .lines()
                        .find_map(|l| {
                            l.to_lowercase()
                                .strip_prefix("content-length:")
                                .map(|v| v.trim().parse::<usize>().unwrap())
                        })
                        .unwrap_or(0);
                    if request.len() >= i + 4 + len {
                        let multipart = header.to_ascii_lowercase().contains("multipart/form-data");
                        break (
                            header,
                            if len == 0 {
                                serde_json::Value::Null
                            } else {
                                let body = &request[i + 4..i + 4 + len];
                                if multipart {
                                    let marker = b"name=\"payload_json\"";
                                    let start = body
                                        .windows(marker.len())
                                        .position(|w| w == marker)
                                        .unwrap();
                                    let body = &body[start..];
                                    let start =
                                        body.windows(4).position(|w| w == b"\r\n\r\n").unwrap() + 4;
                                    let body = &body[start..];
                                    let end = body.windows(4).position(|w| w == b"\r\n--").unwrap();
                                    serde_json::from_slice(&body[..end]).unwrap()
                                } else {
                                    serde_json::from_slice(body).unwrap()
                                }
                            },
                        );
                    }
                }
            };
            records.push((headers.lines().next().unwrap().to_string(), payload));
            if body == "SIMULATE_TIMEOUT_AFTER_RECEIVING" {
                std::thread::sleep(std::time::Duration::from_millis(250));
                continue;
            }
            write!(socket,"HTTP/1.1 {status} OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).unwrap();
        }
        records
    });
    (
        s::HttpBuilder::new("TEST_TOKEN")
            .proxy(format!("http://{address}"))
            .ratelimiter_disabled(true)
            .build(),
        handle,
    )
}
fn message(author: u64) -> String {
    let mut message = s::Message::default();
    message.id = s::MessageId::new(100);
    message.channel_id = s::ChannelId::new(11);
    message.guild_id = Some(s::GuildId::new(1));
    message.author.id = s::UserId::new(author);
    serde_json::to_string(&message).unwrap()
}

fn journal_channel() -> (u16, String) {
    (200, r#"{"id":"11","guild_id":"1"}"#.into())
}

#[test]
fn announcement_reconciliation_matches_attachment_identity_without_expired_signatures_or_binary_data()
 {
    use discord_server_bot::services::publication_state as journal;
    let mut pending = journal_draft("announcements", journal::Write::Edit);
    pending.expected["embeds"][0]["image"] =
        serde_json::json!({"url":"attachment://announcement-hash.png"});
    let mut actual: serde_json::Value = serde_json::from_str(&message(9)).unwrap();
    actual["embeds"] = pending.expected["embeds"].clone();
    actual["embeds"][0]["image"]["url"] = serde_json::json!(
        "https://media.discordapp.net/attachments/11/201/announcement-hash.png?ex=old"
    );
    actual["attachments"] = serde_json::json!([{"id":"201","filename":"announcement-hash.png","url":"https://cdn.discordapp.com/attachments/11/201/announcement-hash.png?ex=new"}]);
    assert!(journal::matches(&pending, &actual, s::UserId::new(9)));
    actual["embeds"][0]["image"]["url"] =
        serde_json::json!("https://external.example/attachments/11/201/announcement-hash.png");
    assert!(!journal::matches(&pending, &actual, s::UserId::new(9)));
}

fn journal_draft(
    module: &str,
    write: discord_server_bot::services::publication_state::Write,
) -> discord_server_bot::services::publication_state::Pending {
    use discord_server_bot::services::publication_state::Pending;
    let doc = Document {
        title: "Test".into(),
        description: "Body".into(),
        ..Default::default()
    };
    Pending {
        guild: s::GuildId::new(1),
        operation: 123,
        key: if module == "links" {
            "links:test".into()
        } else {
            "announcement-123".into()
        },
        module: module.into(),
        action: if write == discord_server_bot::services::publication_state::Write::Edit {
            "edit".into()
        } else {
            "create".into()
        },
        channel: s::ChannelId::new(11),
        write,
        previous: None,
        confirmed_message: None,
        rejected: None,
        expected: serde_json::json!({"content":"", "embeds":doc.embeds("announcements").unwrap()}),
        document: Some(doc),
        invite: None,
    }
}

#[tokio::test]
async fn pending_edit_requires_matching_payload_and_real_guild_ownership() {
    use discord_server_bot::services::{components_v2 as v2, publication_state as journal};
    for case in ["match", "different", "foreign"] {
        let dir = fixture();
        let store = Store::open(dir.path().join("data")).unwrap();
        let guild = s::GuildId::new(1);
        let mut pending = journal_draft("links", journal::Write::Edit);
        let publication = Publication {
            channel: s::ChannelId::new(11),
            message: s::MessageId::new(100),
        };
        pending.previous = Some(publication.clone());
        pending.document = None;
        pending.expected = serde_json::to_value(
            v2::build(
                &Document {
                    title: "desired".into(),
                    ..Default::default()
                },
                "copy",
                vec![],
            )
            .unwrap(),
        )
        .unwrap();
        let mut state = State::default();
        state.publications.insert(pending.key.clone(), publication);
        journal::begin(&store, &mut state, pending.clone()).unwrap();
        let mut actual: serde_json::Value = serde_json::from_str(&message(9)).unwrap();
        actual["flags"] = serde_json::json!(32768);
        actual["components"] = pending.expected["components"].clone();
        actual["components"][0]["id"] = serde_json::json!(44);
        if case == "different" {
            actual["components"][0]["components"][0]["content"] = serde_json::json!("old content");
        }
        let mut responses = vec![if case == "foreign" {
            (200, r#"{"id":"11","guild_id":"2"}"#.into())
        } else {
            journal_channel()
        }];
        if case != "foreign" {
            responses.push((200, actual.to_string()));
        }
        let (http, server) = mock_discord(responses);
        let result = journal::ensure_ready(&store, guild, s::UserId::new(9), &http).await;
        assert_eq!(result.is_ok(), case == "match");
        let after = store.read(guild).unwrap();
        assert_eq!(after.publications[&pending.key].message.get(), 100);
        assert_eq!(after.pending.is_some(), case != "match");
        assert!(
            store
                .read(s::GuildId::new(2))
                .unwrap()
                .publications
                .is_empty()
        );
        server.join().unwrap();
    }
}

#[tokio::test]
async fn explicit_message_lookup_requires_operation_identity_and_delete_reconciliation_requires_known_404()
 {
    use discord_server_bot::services::publication_state as journal;
    for nonce in ["123", "other"] {
        let dir = fixture();
        let store = Store::open(dir.path().join("data")).unwrap();
        let guild = s::GuildId::new(1);
        let pending = journal_draft("announcements", journal::Write::Create);
        let mut state = State::default();
        journal::begin(&store, &mut state, pending.clone()).unwrap();
        let mut actual: serde_json::Value = serde_json::from_str(&message(9)).unwrap();
        actual["nonce"] = serde_json::json!(nonce);
        actual["embeds"] = pending.expected["embeds"].clone();
        let (http, server) = mock_discord(vec![journal_channel(), (200, actual.to_string())]);
        assert_eq!(
            journal::inspect_message(
                &state,
                guild,
                s::UserId::new(9),
                &http,
                s::MessageId::new(100)
            )
            .await
            .is_ok(),
            nonce == "123"
        );
        server.join().unwrap();
    }
    for (status, code) in [(404, 10008), (403, 50001)] {
        let dir = fixture();
        let store = Store::open(dir.path().join("data")).unwrap();
        let guild = s::GuildId::new(1);
        let mut pending = journal_draft("announcements", journal::Write::Delete);
        let publication = Publication {
            channel: s::ChannelId::new(11),
            message: s::MessageId::new(100),
        };
        pending.previous = Some(publication.clone());
        let mut state = State::default();
        state.publications.insert(pending.key.clone(), publication);
        journal::begin(&store, &mut state, pending.clone()).unwrap();
        let (http, server) = mock_discord(vec![
            journal_channel(),
            (
                status,
                serde_json::json!({"code":code,"message":"test"}).to_string(),
            ),
        ]);
        let result = journal::ensure_ready(&store, guild, s::UserId::new(9), &http).await;
        assert_eq!(result.is_ok(), code == 10008);
        let after = store.read(guild).unwrap();
        assert_eq!(after.publications.contains_key(&pending.key), code != 10008);
        assert_eq!(after.pending.is_some(), code != 10008);
        server.join().unwrap();
    }
}

#[tokio::test]
async fn links_edits_omit_noneditable_stickers_and_definitive_rejections_do_not_block_announcements()
 {
    use discord_server_bot::services::publication_state as journal;
    for converted in [false, true] {
        let dir = fixture();
        let config = config::load(dir.path()).unwrap();
        let catalog = Catalog::new(&config);
        let store = Store::open(dir.path().join("data")).unwrap();
        let guild = s::GuildId::new(1);
        let mut state = State::default();
        state.publications.insert(
            "links:test".into(),
            Publication {
                channel: s::ChannelId::new(11),
                message: s::MessageId::new(100),
            },
        );
        store.write(guild, &state).unwrap();
        let mut original: serde_json::Value = serde_json::from_str(&message(9)).unwrap();
        original["flags"] = serde_json::json!(if converted { 32768 } else { 0 });
        let (http, server) = mock_discord(vec![
            (200, original.to_string()),
            (
                403,
                r#"{"code":50080,"message":"Cannot edit stickers within a message"}"#.into(),
            ),
        ]);
        let plan = Plan {
            operation: 123,
            kind: "links",
            action: "publish".into(),
            key: "links:test".into(),
            document: Document {
                title: "Links".into(),
                ..Default::default()
            },
            channel: s::ChannelId::new(11),
            revision: 0,
            content_revision: Some(0),
            delete: false,
        };
        let error = publishing::execute(&store, &catalog, guild, s::UserId::new(9), &http, &plan)
            .await
            .unwrap_err();
        assert_eq!(journal::rejection(&error).unwrap().discord_code, 50080);
        let after = store.read(guild).unwrap();
        assert!(after.pending.is_none() && after.pending_operation.is_none());
        assert_eq!(after.publications[&plan.key].message.get(), 100);
        assert_eq!(
            after.publication_history[0].rejection.unwrap().http_status,
            403
        );
        journal::ensure_ready(&store, guild, s::UserId::new(9), &http)
            .await
            .unwrap(); // No HTTP and no guild-wide blockage.
        let calls = server.join().unwrap();
        let payload = &calls[1].1;
        assert!(payload.get("sticker_ids").is_none());
        assert_eq!(payload["flags"], 32768);
        if converted {
            assert!(
                payload.get("content").is_none()
                    && payload.get("embeds").is_none()
                    && payload.get("poll").is_none()
            );
        } else {
            assert!(payload["content"].is_null());
            assert_eq!(payload["embeds"], serde_json::json!([]));
        }
    }
}

#[tokio::test]
async fn sticker_messages_require_explicit_replacement_and_keep_original_until_success() {
    use discord_server_bot::services::components_v2 as v2;
    for outcome in ["blocked", "success", "rejected", "unknown"] {
        let dir = fixture();
        let config = config::load(dir.path()).unwrap();
        let catalog = Catalog::new(&config);
        let store = Store::open(dir.path().join("data")).unwrap();
        let guild = s::GuildId::new(1);
        let mut state = State::default();
        state.publications.insert(
            "links:test".into(),
            Publication {
                channel: s::ChannelId::new(11),
                message: s::MessageId::new(100),
            },
        );
        store.write(guild, &state).unwrap();
        let mut original: serde_json::Value = serde_json::from_str(&message(9)).unwrap();
        original["sticker_items"] =
            serde_json::json!([{"id":"5","name":"sticker","format_type":1}]);
        let mut created: serde_json::Value = serde_json::from_str(&message(9)).unwrap();
        created["id"] = serde_json::json!("200");
        created["flags"] = serde_json::json!(32768);
        let mut responses = vec![(200, original.to_string())];
        match outcome {
            "success" => responses.push((200, created.to_string())),
            "rejected" => responses.push((
                403,
                r#"{"code":50013,"message":"Missing Permissions"}"#.into(),
            )),
            "unknown" => responses.push((200, "not json".into())),
            _ => (),
        }
        let (http, server) = mock_discord(responses);
        let plan = Plan {
            operation: 123,
            kind: "links",
            action: "publish".into(),
            key: "links:test".into(),
            document: Document {
                title: "Links".into(),
                ..Default::default()
            },
            channel: s::ChannelId::new(11),
            revision: 0,
            content_revision: Some(0),
            delete: false,
        };
        let result = v2::publish_mode(
            &store,
            &catalog,
            guild,
            s::UserId::new(9),
            &http,
            &plan,
            outcome != "blocked",
        )
        .await;
        assert_eq!(result.is_ok(), outcome == "success");
        let after = store.read(guild).unwrap();
        assert_eq!(
            after.publications[&plan.key].message.get(),
            if outcome == "success" { 200 } else { 100 }
        );
        assert_eq!(
            after.archived_publications.len(),
            usize::from(outcome == "success")
        );
        assert_eq!(after.pending.is_some(), outcome == "unknown");
        let calls = server.join().unwrap();
        assert!(
            calls
                .iter()
                .all(|c| !c.0.starts_with("DELETE ") && !c.0.starts_with("PATCH "))
        );
        if outcome != "blocked" {
            assert!(calls[1].0.starts_with("POST "));
            assert_eq!(calls[1].1["nonce"], "123");
            assert_eq!(calls[1].1["enforce_nonce"], true);
        }
        assert!(
            store
                .read(s::GuildId::new(2))
                .unwrap()
                .publications
                .is_empty()
        );
    }
}

#[tokio::test]
async fn confirmed_announcement_receipt_reconciles_after_restart_without_new_post() {
    use discord_server_bot::services::publication_state as journal;
    let dir = fixture();
    let root = dir.path().join("data");
    let guild = s::GuildId::new(1);
    let pending = journal_draft("announcements", journal::Write::Create);
    {
        let store = Store::open(root.clone()).unwrap();
        let mut state = State::default();
        journal::begin(&store, &mut state, pending.clone()).unwrap();
        journal::received(&store, &mut state, guild, s::MessageId::new(100)).unwrap();
    }
    let store = Store::open(root).unwrap();
    let mut actual: serde_json::Value = serde_json::from_str(&message(9)).unwrap();
    actual["embeds"] = pending.expected["embeds"].clone();
    let (http, server) = mock_discord(vec![journal_channel(), (200, actual.to_string())]);
    journal::ensure_ready(&store, guild, s::UserId::new(9), &http)
        .await
        .unwrap();
    let state = store.read(guild).unwrap();
    assert!(state.pending.is_none());
    assert_eq!(state.publications[&pending.key].message.get(), 100);
    assert_eq!(state.documents[&pending.key].description, "Body");
    assert_eq!(state.publication_history[0].outcome, "reconciled_success");
    assert!(server.join().unwrap()[0].0.starts_with("GET "));
    let path = dir.path().join("data/guilds/1");
    assert!(fs::read_dir(path).unwrap().any(|p| {
        p.unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with("state.pre-reconcile-")
    }));
    assert!(
        store
            .read(s::GuildId::new(2))
            .unwrap()
            .publications
            .is_empty()
    );
}

#[tokio::test]
async fn post_with_unknown_response_recovers_only_positive_nonce_match_not_empty_search() {
    use discord_server_bot::services::publication_state as journal;
    for found in [false, true] {
        let dir = fixture();
        let store = Store::open(dir.path().join("data")).unwrap();
        let guild = s::GuildId::new(1);
        let pending = journal_draft("announcements", journal::Write::Create);
        let mut state = State::default();
        journal::begin(&store, &mut state, pending.clone()).unwrap();
        let mut actual: serde_json::Value = serde_json::from_str(&message(9)).unwrap();
        actual["embeds"] = pending.expected["embeds"].clone();
        actual["nonce"] = serde_json::json!("123");
        let body = if found {
            serde_json::json!([actual])
        } else {
            serde_json::json!([])
        };
        let (http, server) = mock_discord(vec![journal_channel(), (200, body.to_string())]);
        let result = journal::ensure_ready(&store, guild, s::UserId::new(9), &http).await;
        assert_eq!(result.is_ok(), found);
        assert_eq!(store.read(guild).unwrap().pending.is_some(), !found);
        let calls = server.join().unwrap();
        assert_eq!(calls.len(), 2);
        assert!(calls.iter().all(|c| c.0.starts_with("GET ")));
        if !found {
            assert!(result.unwrap_err().to_string().contains("Announcements"));
            assert!(store.read(guild).unwrap().publications.is_empty());
        }
    }
}

#[tokio::test]
async fn legacy_pending_is_not_cleared_by_existing_message_and_requires_attested_rejection() {
    use discord_server_bot::services::publication_state as journal;
    let dir = fixture();
    let store = Store::open(dir.path().join("data")).unwrap();
    let guild = s::GuildId::new(1);
    let mut state = State {
        pending: Some("links:123".into()),
        ..Default::default()
    };
    state.publications.insert(
        "links".into(),
        Publication {
            channel: s::ChannelId::new(11),
            message: s::MessageId::new(100),
        },
    );
    store.write(guild, &state).unwrap();
    let (http, server) = mock_discord(vec![
        journal_channel(),
        (200, message(9)),
        journal_channel(),
        (200, message(9)),
    ]);
    assert!(
        journal::ensure_ready(&store, guild, s::UserId::new(9), &http)
            .await
            .is_err()
    );
    assert_eq!(store.read(guild).unwrap().pending, state.pending);
    assert!(
        journal::attest_rejection(
            &store,
            &mut state,
            guild,
            s::UserId::new(9),
            &http,
            "links:WRONG",
            journal::Rejection {
                http_status: 403,
                discord_code: 50080
            }
        )
        .await
        .is_err()
    );
    journal::attest_rejection(
        &store,
        &mut state,
        guild,
        s::UserId::new(9),
        &http,
        "links:123",
        journal::Rejection {
            http_status: 403,
            discord_code: 50080,
        },
    )
    .await
    .unwrap();
    assert!(state.pending.is_none());
    assert_eq!(state.publications["links"].message.get(), 100);
    assert_eq!(state.publication_history[0].pending, "links:123");
    assert_eq!(server.join().unwrap().len(), 4);
    assert!(
        store
            .read(s::GuildId::new(2))
            .unwrap()
            .publication_history
            .is_empty()
    );
}

#[tokio::test]
async fn durable_rejection_is_reconciled_but_foreign_or_inconsistent_journals_are_not() {
    use discord_server_bot::services::publication_state as journal;
    for invalid in [false, true] {
        let dir = fixture();
        let store = Store::open(dir.path().join("data")).unwrap();
        let guild = s::GuildId::new(1);
        let mut pending = journal_draft("announcements", journal::Write::Create);
        pending.rejected = Some(journal::Rejection {
            http_status: 400,
            discord_code: 50035,
        });
        if invalid {
            pending.guild = s::GuildId::new(2);
        }
        let mut state = State {
            pending: Some("announcement-123:123".into()),
            pending_operation: Some(pending),
            ..Default::default()
        };
        store.write(guild, &state).unwrap();
        let http = s::Http::new("TEST_TOKEN");
        let result = journal::ensure_ready(&store, guild, s::UserId::new(9), &http).await;
        assert_eq!(result.is_err(), invalid);
        state = store.read(guild).unwrap();
        assert_eq!(state.pending.is_some(), invalid);
    }
}

#[tokio::test]
async fn diagnostics_distinguish_unsupported_stickers_and_redact_free_form_api_details() {
    use discord_server_bot::core::diagnostics;
    let message = diagnostics::http_message(403, 50080);
    assert!(message.contains("50080"));
    assert!(message.contains("stickers"));
    assert_ne!(message, diagnostics::http_message(403, 50013));
    let (http,server)=mock_discord(vec![(400,r#"{"code":50035,"message":"secret https://private.example/?token=secret","errors":{"embeds":{"0":{"description":{"_errors":[{"code":"BASE_TYPE_MAX_LENGTH","message":"secret data"}]}}}}}"#.into())]);
    let error = s::ChannelId::new(11)
        .message(&http, s::MessageId::new(100))
        .await
        .unwrap_err();
    let safe = diagnostics::sanitized_http_response(&error).to_string();
    assert!(!safe.contains("secret") && !safe.contains("private.example"));
    assert!(safe.contains("BASE_TYPE_MAX_LENGTH"));
    assert!(safe.contains("embeds.0.description"));
    server.join().unwrap();
}

#[tokio::test]
async fn announcement_timeout_after_post_keeps_journal_and_never_reposts() {
    use discord_server_bot::services::publication_state as journal;
    let dir = fixture();
    let config = config::load(dir.path()).unwrap();
    let catalog = Catalog::new(&config);
    let store = Store::open(dir.path().join("data")).unwrap();
    let guild = s::GuildId::new(1);
    let (base, server) = mock_discord(vec![
        (200, announcement_guild("Guild", None)),
        (200, "SIMULATE_TIMEOUT_AFTER_RECEIVING".into()),
    ]);
    let http = s::HttpBuilder::new("TEST_TOKEN")
        .proxy(base.proxy.clone().unwrap())
        .ratelimiter_disabled(true)
        .client(
            reqwest::Client::builder()
                .timeout(std::time::Duration::from_millis(100))
                .build()
                .unwrap(),
        )
        .build();
    let plan = Plan {
        operation: 123,
        kind: "announcements",
        action: "create".into(),
        key: "announcement-123".into(),
        document: Document {
            title: "Test".into(),
            description: "Body".into(),
            ..Default::default()
        },
        channel: s::ChannelId::new(11),
        revision: 0,
        content_revision: Some(0),
        delete: false,
    };
    let error = publishing::execute(&store, &catalog, guild, s::UserId::new(9), &http, &plan)
        .await
        .unwrap_err();
    assert!(journal::rejection(&error).is_none());
    let saved = store.read(guild).unwrap();
    assert!(saved.pending.is_some());
    assert!(saved.pending_operation.is_some());
    assert!(saved.publications.is_empty());
    let calls = server.join().unwrap();
    assert_eq!(calls.len(), 2);
    assert_eq!(calls[1].1["nonce"], "123");
    assert_eq!(calls[1].1["enforce_nonce"], true);
    let (lookup, read_server) = mock_discord(vec![journal_channel(), (200, "[]".into())]);
    assert!(
        journal::ensure_ready(&store, guild, s::UserId::new(9), &lookup)
            .await
            .is_err()
    );
    assert!(read_server.join().unwrap()[0].0.starts_with("GET "));
    assert_eq!(store.read(guild).unwrap().pending, saved.pending);
}

#[tokio::test]
async fn announcement_rejected_create_archives_failure_and_preserves_other_publications() {
    let dir = fixture();
    let config = config::load(dir.path()).unwrap();
    let catalog = Catalog::new(&config);
    let store = Store::open(dir.path().join("data")).unwrap();
    let guild = s::GuildId::new(1);
    let mut state = State::default();
    state.publications.insert(
        "links:test".into(),
        Publication {
            channel: s::ChannelId::new(11),
            message: s::MessageId::new(100),
        },
    );
    store.write(guild, &state).unwrap();
    let (http, server) = mock_discord(vec![
        (200, announcement_guild("Guild", None)),
        (
            403,
            r#"{"code":50013,"message":"Missing Permissions"}"#.into(),
        ),
    ]);
    let plan = Plan {
        operation: 123,
        kind: "announcements",
        action: "create".into(),
        key: "announcement-123".into(),
        document: Document {
            title: "Test".into(),
            description: "Body".into(),
            ..Default::default()
        },
        channel: s::ChannelId::new(11),
        revision: 0,
        content_revision: Some(0),
        delete: false,
    };
    assert!(
        publishing::execute(&store, &catalog, guild, s::UserId::new(9), &http, &plan)
            .await
            .is_err()
    );
    let state = store.read(guild).unwrap();
    assert!(state.pending.is_none());
    assert_eq!(state.publications["links:test"].message.get(), 100);
    assert!(!state.publications.contains_key("announcement-123"));
    assert_eq!(
        state.publication_history[0].rejection.unwrap().discord_code,
        50013
    );
    server.join().unwrap();
}
fn announcement_guild(name: &str, icon: Option<&str>) -> String {
    serde_json::json!({
        "id":"1", "name":name, "icon":icon, "owner_id":"99", "verification_level":0,
        "default_message_notifications":0, "explicit_content_filter":0, "roles":[],
        "emojis":[], "features":[], "mfa_level":0, "system_channel_flags":0,
        "premium_tier":0, "preferred_locale":"pt-BR", "nsfw_level":0,
        "stickers":[], "premium_progress_bar_enabled":false
    })
    .to_string()
}

#[tokio::test]
async fn profile_defer_then_followup_use_different_routes_and_ack_only_once() {
    use discord_server_bot::core::interactions::{self as i, Responses, Stage};
    let (http, server) = mock_discord(vec![(204, String::new()), (200, message(9))]);
    http.set_application_id(s::ApplicationId::new(99));
    let responses = Responses::default();
    let id = s::InteractionId::new(1000);
    assert!(
        i::followup_request(
            &http,
            id,
            "TEST_TOKEN",
            Stage::Progress,
            false,
            "Não enviar".into()
        )
        .await
        .is_err()
    );
    i::initial(
        &http,
        &responses,
        id,
        "TEST_TOKEN",
        Stage::SlashDefer,
        s::CreateInteractionResponse::Defer(
            s::CreateInteractionResponseMessage::new().ephemeral(true),
        ),
    )
    .await
    .unwrap();
    i::followup_request(
        &http,
        id,
        "TEST_TOKEN",
        Stage::Progress,
        true,
        "Recebido".into(),
    )
    .await
    .unwrap();
    let duplicate = i::initial(
        &http,
        &responses,
        id,
        "TEST_TOKEN",
        Stage::SlashDefer,
        s::CreateInteractionResponse::Acknowledge,
    )
    .await
    .unwrap_err();
    assert!(duplicate.downcast_ref::<i::Failure>().unwrap().duplicate);
    let records = server.join().unwrap();
    assert_eq!(records.len(), 2);
    assert!(
        records[0]
            .0
            .contains("POST /api/v10/interactions/1000/TEST_TOKEN/callback")
    );
    assert_eq!(records[0].1["type"], 5);
    assert_eq!(records[0].1["data"]["flags"], 64);
    assert!(
        records[1]
            .0
            .contains("POST /api/v10/webhooks/99/TEST_TOKEN")
    );
    assert_eq!(records[1].1["flags"], 64);
    assert_eq!(
        records[1].1["allowed_mentions"]["parse"],
        serde_json::json!([])
    );
}

#[tokio::test]
async fn unknown_interaction_is_tagged_with_callback_stage_and_never_retried() {
    use discord_server_bot::core::interactions::{self as i, Responses, Stage};
    let (http, server) = mock_discord(vec![(
        404,
        serde_json::json!({"code":10062,"message":"PRIVATE INTERACTION TOKEN"}).to_string(),
    )]);
    let responses = Responses::default();
    let error = i::initial(
        &http,
        &responses,
        s::InteractionId::new(1000),
        "PRIVATE_TOKEN",
        Stage::SlashDefer,
        s::CreateInteractionResponse::Defer(
            s::CreateInteractionResponseMessage::new().ephemeral(true),
        ),
    )
    .await
    .unwrap_err();
    let failure = error.downcast_ref::<i::Failure>().unwrap();
    assert_eq!(failure.code, Some(10062));
    assert_eq!(failure.stage, Stage::SlashDefer);
    assert_eq!(
        failure.stage.route(),
        "POST /interactions/{id}/{token}/callback"
    );
    assert!(!format!("{error:?}").contains("PRIVATE"));
    assert!(!i::may_report_after(false, 1000, &error));
    assert!(!i::may_report_after(true, 1000, &error));
    assert!(
        i::initial(
            &http,
            &responses,
            s::InteractionId::new(1000),
            "PRIVATE_TOKEN",
            Stage::SlashDefer,
            s::CreateInteractionResponse::Acknowledge
        )
        .await
        .is_err()
    );
    assert_eq!(server.join().unwrap().len(), 1);
}

#[tokio::test]
async fn confirmations_ack_cancel_and_submit_once_and_expired_ack_cannot_confirm() {
    use discord_server_bot::core::interactions::{self as i, Responses};
    let (http, server) = mock_discord(vec![
        (204, String::new()),
        (204, String::new()),
        (
            404,
            serde_json::json!({"code":10062,"message":"Unknown interaction"}).to_string(),
        ),
    ]);
    let responses = Responses::default();
    assert!(
        !i::confirmation(
            &http,
            &responses,
            s::InteractionId::new(1001),
            "TEST",
            "confirm:1",
            "cancel:1"
        )
        .await
        .unwrap()
    );
    assert!(
        i::confirmation(
            &http,
            &responses,
            s::InteractionId::new(1002),
            "TEST",
            "confirm:1",
            "confirm:1"
        )
        .await
        .unwrap()
    );
    assert!(
        i::confirmation(
            &http,
            &responses,
            s::InteractionId::new(1002),
            "TEST",
            "confirm:1",
            "confirm:1"
        )
        .await
        .is_err()
    );
    let expired = i::confirmation(
        &http,
        &responses,
        s::InteractionId::new(1003),
        "TEST",
        "confirm:1",
        "confirm:1",
    )
    .await
    .unwrap_err();
    assert!(i::may_report_after(true, 1000, &expired));
    let records = server.join().unwrap();
    assert_eq!(records.len(), 3);
    assert!(records.iter().all(|r| r.1["type"] == 6));
    assert!(records.iter().all(|r| !r.0.contains("/members/")));
}

#[tokio::test]
async fn concurrent_consumers_cannot_acknowledge_the_same_interaction_twice() {
    use discord_server_bot::core::interactions::{self as i, Responses, Stage};
    let (http, server) = mock_discord(vec![(204, String::new())]);
    let responses = Responses::default();
    let (one, two) = tokio::join!(
        i::initial(
            &http,
            &responses,
            s::InteractionId::new(1000),
            "TEST",
            Stage::ConfirmationAck,
            s::CreateInteractionResponse::Acknowledge
        ),
        i::initial(
            &http,
            &responses,
            s::InteractionId::new(1000),
            "TEST",
            Stage::ConfirmationAck,
            s::CreateInteractionResponse::Acknowledge
        )
    );
    assert_eq!(usize::from(one.is_ok()) + usize::from(two.is_ok()), 1);
    assert_eq!(server.join().unwrap().len(), 1);
}

#[tokio::test]
async fn modal_open_and_submit_have_separate_initial_responses_and_final_expiry_preserves_outcome()
{
    use discord_server_bot::core::interactions::{self as i, Responses, Stage};
    let (http, server) = mock_discord(vec![
        (204, String::new()),
        (204, String::new()),
        (
            404,
            serde_json::json!({"code":10015,"message":"PRIVATE_TOKEN"}).to_string(),
        ),
    ]);
    http.set_application_id(s::ApplicationId::new(99));
    let responses = Responses::default();
    i::initial(
        &http,
        &responses,
        s::InteractionId::new(2000),
        "TEST",
        Stage::ModalOpen,
        s::CreateInteractionResponse::Modal(
            s::CreateModal::new("modal:2000", "Perfil").components(vec![
                s::CreateActionRow::InputText(s::CreateInputText::new(
                    s::InputTextStyle::Short,
                    "Valor",
                    "value",
                )),
            ]),
        ),
    )
    .await
    .unwrap();
    i::initial(
        &http,
        &responses,
        s::InteractionId::new(2001),
        "TEST",
        Stage::ModalSubmitAck,
        s::CreateInteractionResponse::Acknowledge,
    )
    .await
    .unwrap();
    let error = i::followup_request(
        &http,
        s::InteractionId::new(1000),
        "TEST",
        Stage::Result,
        true,
        "Perfil atualizado".into(),
    )
    .await
    .unwrap_err();
    assert!(error.to_string().contains("perfil já foi processado"));
    assert!(!error.to_string().contains("antes de executar"));
    assert!(!i::may_report_after(true, 1000, &error));
    let records = server.join().unwrap();
    assert_eq!(records[0].1["type"], 9);
    assert_eq!(records[1].1["type"], 6);
    assert!(records[2].0.contains("/webhooks/99/TEST"));
}

#[test]
fn interaction_deadline_and_permission_diagnostics_are_distinct() {
    use discord_server_bot::core::{diagnostics, interactions};
    let epoch = 1_420_070_400_000;
    let created = 1_800_000_000_000u64;
    let id = (created - epoch) << 22;
    assert_eq!(interactions::age_at(id, created + 2999), Some(2999));
    assert_eq!(interactions::age_at(id, created + 3001), Some(3001));
    assert_eq!(interactions::age_at(id, created - 1), None);
    let expired = diagnostics::http_message(404, 10062);
    assert!(expired.contains("Interação"));
    assert!(!expired.contains("pending"));
    assert!(diagnostics::http_message(403, 50001).contains("negou acesso"));
    assert!(diagnostics::http_message(403, 50013).contains("permissão insuficiente"));
    assert!(diagnostics::http_message(400, 40060).contains("já respondida"));
    assert_eq!(
        diagnostics::discord_route(
            "POST",
            "https://discord.com/api/v10/interactions/123/PRIVATE_TOKEN/callback?private=SECRET"
        ),
        "POST /interactions/{id}/{token}/callback"
    );
    assert_eq!(
        diagnostics::discord_route("PATCH", "https://discord.com/api/v10/guilds/1/members/@me"),
        "PATCH /guilds/{guild}/members/@me"
    );
}

#[tokio::test]
async fn permission_failures_and_failed_error_notifications_keep_the_correct_scope() {
    use discord_server_bot::core::interactions::{self as i, Responses, Stage};
    for (code, expected) in [(50001, "negou acesso"), (50013, "permissão insuficiente")] {
        let (http, server) = mock_discord(vec![(
            403,
            serde_json::json!({"code":code,"message":"PRIVATE_TOKEN"}).to_string(),
        )]);
        let error = i::initial(
            &http,
            &Responses::default(),
            s::InteractionId::new(2000),
            "TEST",
            Stage::ConfirmationAck,
            s::CreateInteractionResponse::Acknowledge,
        )
        .await
        .unwrap_err();
        assert_eq!(error.downcast_ref::<i::Failure>().unwrap().code, Some(code));
        assert!(error.to_string().contains(expected));
        assert!(!error.to_string().contains("expirada"));
        assert_eq!(server.join().unwrap().len(), 1);
    }
    let (http, server) = mock_discord(vec![(
        404,
        serde_json::json!({"code":10015,"message":"PRIVATE_TOKEN"}).to_string(),
    )]);
    http.set_application_id(s::ApplicationId::new(99));
    let error = i::followup_request(
        &http,
        s::InteractionId::new(1000),
        "TEST",
        Stage::ErrorNotice,
        true,
        "Diagnóstico original".into(),
    )
    .await
    .unwrap_err();
    assert!(error.to_string().contains("erro original nos logs"));
    assert!(!error.to_string().contains("antes de executar"));
    assert!(!i::may_report_after(true, 1000, &error));
    assert_eq!(server.join().unwrap().len(), 1);
}

#[tokio::test]
async fn announcement_upload_multipart_edit_restart_replace_and_remove_preserve_message_id() {
    use discord_server_bot::services::announcements::media::{self, ImageChange, PublishedImage};
    let dir = fixture();
    let config = config::load(dir.path()).unwrap();
    let catalog = Catalog::new(&config);
    let mut store = Store::open(dir.path().join("data")).unwrap();
    let gid = s::GuildId::new(1);
    let mut encoded = Cursor::new(Vec::new());
    image::DynamicImage::new_rgba8(32, 32)
        .write_to(&mut encoded, image::ImageFormat::Png)
        .unwrap();
    let first = media::validate(
        encoded.get_ref(),
        "first.png",
        Some("image/png"),
        media::MAX_UPLOAD_BYTES,
    )
    .unwrap();
    let mut encoded = Cursor::new(Vec::new());
    image::DynamicImage::new_rgba8(64, 64)
        .write_to(&mut encoded, image::ImageFormat::Png)
        .unwrap();
    let second = media::validate(
        encoded.get_ref(),
        "second.png",
        Some("image/png"),
        media::MAX_UPLOAD_BYTES,
    )
    .unwrap();
    let first_name = first.attachment().filename;
    let second_name = second.attachment().filename;
    let files = |entries: &[(u64, &str)]| {
        let mut value: serde_json::Value = serde_json::from_str(&message(9)).unwrap();
        value["attachments"] = serde_json::Value::Array(entries.iter().map(|(id,name)| serde_json::json!({
            "id":id.to_string(),"filename":name,"size":1024,"url":format!("https://cdn.discordapp.com/attachments/11/100/{name}"),"proxy_url":format!("https://media.discordapp.net/attachments/11/100/{name}")
        })).collect());
        value.to_string()
    };
    let outcomes = [
        files(&[(501, &first_name)]),
        files(&[(501, &first_name), (888, "other.txt")]),
        files(&[(502, &second_name), (888, "other.txt")]),
        files(&[(888, "other.txt")]),
        files(&[(503, &first_name), (888, "other.txt")]),
        files(&[(888, "other.txt")]),
    ];
    let mut responses = vec![];
    for (index, result) in outcomes.iter().enumerate() {
        responses.push((200, announcement_guild("Receita Federal", None)));
        if index != 0 {
            let previous = if index == 1 {
                files(&[(501, &first_name), (888, "other.txt")])
            } else {
                outcomes[index - 1].clone()
            };
            responses.push((200, previous));
        }
        responses.push((200, result.clone()));
    }
    let (http, server) = mock_discord(responses);
    let changes = [
        ImageChange::Upload(first.clone()),
        ImageChange::Keep,
        ImageChange::Upload(second),
        ImageChange::External("https://example.org/banner.png".into()),
        ImageChange::Upload(first),
        ImageChange::Remove,
    ];
    for (index, image) in changes.iter().enumerate() {
        let state = store.read(gid).unwrap();
        let base = state
            .documents
            .get("announcement-123")
            .cloned()
            .unwrap_or(Document {
                title: "Aviso".into(),
                description: "Texto inicial".into(),
                ..Default::default()
            });
        let plan = Plan {
            operation: 123,
            kind: "announcements",
            action: if index == 0 { "create" } else { "edit" }.into(),
            key: "announcement-123".into(),
            document: Document {
                description: format!("Edição {index}"),
                ..base
            },
            channel: s::ChannelId::new(11),
            revision: state.revision,
            content_revision: None,
            delete: false,
        };
        publishing::execute_request(
            &store,
            &catalog,
            gid,
            s::UserId::new(9),
            &http,
            publishing::Request { plan: &plan, image },
        )
        .await
        .unwrap();
        let saved = store.read(gid).unwrap();
        assert_eq!(saved.publications[&plan.key].message.get(), 100);
        assert_eq!(
            saved.documents[&plan.key].description,
            format!("Edição {index}")
        );
        match index {
            0 | 1 => assert!(
                matches!(saved.announcement_images[&plan.key], PublishedImage::Attachment {id,..} if id.get()==501)
            ),
            2 => assert!(
                matches!(saved.announcement_images[&plan.key], PublishedImage::Attachment {id,..} if id.get()==502)
            ),
            3 => assert!(matches!(
                saved.announcement_images[&plan.key],
                PublishedImage::External { .. }
            )),
            4 => assert!(
                matches!(saved.announcement_images[&plan.key], PublishedImage::Attachment {id,..} if id.get()==503)
            ),
            _ => {
                assert!(!saved.announcement_images.contains_key(&plan.key));
                assert!(saved.documents[&plan.key].image.is_empty());
            }
        }
        drop(store);
        store = Store::open(dir.path().join("data")).unwrap();
    }
    let records = server.join().unwrap();
    let writes = records
        .iter()
        .filter(|r| r.0.starts_with("POST ") || r.0.starts_with("PATCH "))
        .collect::<Vec<_>>();
    assert_eq!(writes.len(), 6);
    assert!(writes[0].0.contains("POST /api/v10/channels/11/messages"));
    for update in &writes[1..] {
        assert!(update.0.contains("PATCH /api/v10/channels/11/messages/100"));
    }
    assert_eq!(
        writes[0].1["embeds"][0]["image"]["url"],
        format!("attachment://{first_name}")
    );
    assert_eq!(
        writes[1].1["attachments"],
        serde_json::json!([{"id":"501"},{"id":"888"}])
    );
    assert_eq!(writes[2].1["attachments"][0]["id"], "888");
    assert_eq!(writes[2].1["attachments"][1]["filename"], second_name);
    assert_eq!(
        writes[3].1["attachments"],
        serde_json::json!([{"id":"888"}])
    );
    assert_eq!(
        writes[3].1["embeds"][0]["image"]["url"],
        "https://example.org/banner.png"
    );
    assert!(writes[5].1["embeds"][0].get("image").is_none());
    assert_eq!(
        writes[5].1["attachments"],
        serde_json::json!([{"id":"888"}])
    );
    assert!(
        store
            .read(s::GuildId::new(2))
            .unwrap()
            .announcement_images
            .is_empty()
    );
}

#[tokio::test]
async fn managed_publications_create_edit_remove_and_reject_duplicate_confirmation() {
    for kind in ["rules", "links", "announcements"] {
        let dir = fixture();
        let config = config::load(dir.path()).unwrap();
        let catalog = Catalog::new(&config);
        let store = Store::open(dir.path().join("data")).unwrap();
        let key = if kind == "announcements" {
            "announcement-123"
        } else if kind == "links" {
            "links:important"
        } else {
            "rules"
        };
        let doc = if kind == "rules" {
            toml::from_str(include_str!("../templates/global/rules/default.toml")).unwrap()
        } else {
            Document {
                title: "Original".into(),
                description: "Body".into(),
                entries: if kind == "links" {
                    vec![link("discord", "https://discord.gg/EXEMPLO")]
                } else {
                    vec![]
                },
                ..Default::default()
            }
        };
        let mut plan = Plan {
            operation: 123,
            kind,
            action: if kind == "announcements" {
                "create".into()
            } else {
                "publish".into()
            },
            key: key.into(),
            document: doc,
            channel: s::ChannelId::new(11),
            revision: 0,
            content_revision: Some(0),
            delete: false,
        };
        let mut responses = vec![
            (200, message(9)),
            (200, message(9)),
            (200, message(9)),
            (200, message(9)),
            (204, String::new()),
        ];
        if kind == "announcements" {
            responses.insert(
                0,
                (
                    200,
                    announcement_guild("Receita Federal", Some("0123456789abcdef0123456789abcdef")),
                ),
            );
            responses.insert(2, (200, announcement_guild("Servidor renomeado", None)));
        }
        let (http, server) = mock_discord(responses);
        let gid = s::GuildId::new(1);
        let bot = s::UserId::new(9);
        publishing::execute(&store, &catalog, gid, bot, &http, &plan)
            .await
            .unwrap();
        let state = store.read(gid).unwrap();
        assert_eq!(state.publications[key].message.get(), 100);
        assert_eq!(state.documents.contains_key(key), kind == "announcements");
        if kind == "announcements" {
            assert_eq!(
                state.documents[key].footer,
                "Comunicado oficial • Receita Federal • Ograda Solutions Bot"
            );
            assert!(state.documents[key].thumbnail.contains("/icons/1/"));
        }
        if kind == "links" {
            assert_eq!(state.link_invites[key].url, "https://discord.gg/EXEMPLO");
            plan.document.entries[0].url = "https://discord.com/invite/OUTRO".into();
            plan.operation = 124;
        }
        plan.revision = state.revision;
        plan.document.title = "Updated".into();
        if kind == "announcements" {
            plan.action = "edit".into();
        }
        publishing::execute(&store, &catalog, gid, bot, &http, &plan)
            .await
            .unwrap();
        assert!(
            publishing::execute(&store, &catalog, gid, bot, &http, &plan)
                .await
                .is_err()
        ); // Stale duplicate; no HTTP.
        assert_eq!(
            store.read(gid).unwrap().publications[key].message.get(),
            100
        );
        if kind == "announcements" {
            assert_eq!(
                store.read(gid).unwrap().documents[key].footer,
                "Comunicado oficial • Servidor renomeado • Ograda Solutions Bot"
            );
            assert!(store.read(gid).unwrap().documents[key].thumbnail.is_empty());
        }
        if kind == "links" {
            assert_eq!(
                store.read(gid).unwrap().link_invites[key].url,
                "https://discord.com/invite/OUTRO"
            );
            assert_eq!(
                store.read(gid).unwrap().link_invites[key].custom_id,
                "links-copy:124"
            );
        }
        plan.revision = store.read(gid).unwrap().revision;
        plan.action = "remove".into();
        plan.delete = true;
        publishing::execute(&store, &catalog, gid, bot, &http, &plan)
            .await
            .unwrap();
        let records = server.join().unwrap();
        if kind == "announcements" {
            assert_eq!(
                records
                    .iter()
                    .filter(|r| r.0.contains("GET /api/v10/guilds/1 "))
                    .count(),
                2
            );
        }
        let records = records
            .into_iter()
            .filter(|r| !r.0.contains("GET /api/v10/guilds/1 "))
            .collect::<Vec<_>>();
        assert!(records[0].0.starts_with("POST "));
        assert!(
            records[2]
                .0
                .contains("PATCH /api/v10/channels/11/messages/100")
        );
        assert!(records[4].0.starts_with("DELETE "));
        assert_eq!(
            records[0].1["allowed_mentions"]["parse"],
            serde_json::json!([])
        );
        if kind != "links" {
            assert_eq!(records[2].1["embeds"][0]["title"], "Updated");
        }
        if kind == "announcements" {
            assert_eq!(
                records[0].1["embeds"][0]["footer"]["text"],
                "Comunicado oficial • Receita Federal • Ograda Solutions Bot"
            );
            assert!(
                records[0].1["embeds"][0]["thumbnail"]["url"]
                    .as_str()
                    .unwrap()
                    .contains("/icons/1/")
            );
            assert_eq!(
                records[2].1["embeds"][0]["footer"]["text"],
                "Comunicado oficial • Servidor renomeado • Ograda Solutions Bot"
            );
            assert!(records[2].1["embeds"][0].get("thumbnail").is_none());
        }
        if kind == "links" {
            use discord_server_bot::services::components_v2::IS_COMPONENTS_V2;
            assert_eq!(records[0].1["flags"], IS_COMPONENTS_V2);
            assert!(records[0].1.get("embeds").is_none());
            assert!(records[0].1.get("content").is_none());
            assert_eq!(records[2].1["flags"], IS_COMPONENTS_V2);
            assert_eq!(records[2].1["embeds"], serde_json::json!([]));
            assert!(records[2].1["content"].is_null());
            let children = records[2].1["components"][0]["components"]
                .as_array()
                .unwrap();
            assert!(children[0]["content"].as_str().unwrap().contains("Updated"));
            let row = children.iter().find(|c| c["type"] == 1).unwrap();
            assert_eq!(row["components"][0]["custom_id"], "links-copy:124");
            assert_eq!(
                row["components"][1]["custom_id"],
                discord_server_bot::services::links::INVITE_FRIENDS_ID
            );
            assert!(row["components"][1].get("url").is_none());
            assert!(!store.read(gid).unwrap().link_invites.contains_key(key));
        }
        if kind == "rules" {
            for request in [&records[0], &records[2]] {
                assert_eq!(request.1["embeds"].as_array().unwrap().len(), 1);
                let embed = &request.1["embeds"][0];
                assert!(
                    embed
                        .get("fields")
                        .is_none_or(|v| v.as_array().unwrap().is_empty())
                );
                assert!(embed.get("footer").is_none());
                for entry in &plan.document.entries {
                    assert!(
                        embed["description"]
                            .as_str()
                            .unwrap()
                            .contains(&entry.description)
                    );
                }
            }
        }
        assert!(!store.read(gid).unwrap().publications.contains_key(key));
        assert!(
            store
                .read(s::GuildId::new(2))
                .unwrap()
                .publications
                .is_empty()
        );
        assert!(dir.path().join("guilds/defaults/rules.toml").is_file());
        assert!(dir.path().join("guilds/defaults/links.toml").is_file());
    }
}

#[tokio::test]
async fn links_publish_recreates_confirmed_missing_message_and_archives_previous_reference() {
    let dir = fixture();
    let config = config::load(dir.path()).unwrap();
    let catalog = Catalog::new(&config);
    let store = Store::open(dir.path().join("data")).unwrap();
    let gid = s::GuildId::new(1);
    let mut state = State::default();
    state.publications.insert(
        "links:important".into(),
        Publication {
            channel: s::ChannelId::new(11),
            message: s::MessageId::new(100),
        },
    );
    store.write(gid, &state).unwrap();
    let toml_path = dir.path().join("guilds/defaults/links.toml");
    let original_toml = fs::read(&toml_path).unwrap();
    let plan = Plan {
        operation: 987,
        kind: "links",
        action: "publish".into(),
        key: "links:important".into(),
        document: Document {
            title: "Links".into(),
            entries: vec![link("discord", "https://discord.gg/EXEMPLO")],
            ..Default::default()
        },
        channel: s::ChannelId::new(11),
        revision: 0,
        content_revision: Some(0),
        delete: false,
    };
    let mut created: s::Message = serde_json::from_str(&message(9)).unwrap();
    created.id = s::MessageId::new(200);
    let missing = r#"{"code":10008,"message":"Unknown Message"}"#.to_string();
    let (http, server) = mock_discord(vec![
        (404, missing),
        (200, serde_json::to_string(&created).unwrap()),
    ]);
    let result = publishing::execute(&store, &catalog, gid, s::UserId::new(9), &http, &plan)
        .await
        .unwrap();
    assert!(result.contains("nova publicação"));
    let saved = store.read(gid).unwrap();
    assert_eq!(saved.publications[&plan.key].message.get(), 200);
    assert_eq!(saved.archived_publications.len(), 1);
    assert_eq!(
        saved.archived_publications[0].publication.message.get(),
        100
    );
    assert_eq!(saved.archived_publications[0].key, plan.key);
    assert_eq!(saved.link_invites[&plan.key].custom_id, "links-copy:987");
    assert!(saved.pending.is_none());
    assert!(
        publishing::execute(&store, &catalog, gid, s::UserId::new(9), &http, &plan)
            .await
            .is_err()
    );
    let records = server.join().unwrap();
    assert_eq!(records.len(), 2);
    assert!(
        records[0]
            .0
            .contains("GET /api/v10/channels/11/messages/100")
    );
    assert!(records[1].0.contains("POST /api/v10/channels/11/messages"));
    assert!(records[1].1.get("embeds").is_none());
    assert_eq!(records[1].1["components"][0]["type"], 17);
    assert_eq!(fs::read(&toml_path).unwrap(), original_toml);
    assert!(
        store
            .read(s::GuildId::new(2))
            .unwrap()
            .publications
            .is_empty()
    );
}

#[tokio::test]
async fn links_publish_does_not_recreate_on_permission_channel_or_server_errors() {
    for (status, code) in [(403, 50013), (404, 10003), (500, 0)] {
        let dir = fixture();
        let config = config::load(dir.path()).unwrap();
        let catalog = Catalog::new(&config);
        let store = Store::open(dir.path().join("data")).unwrap();
        let gid = s::GuildId::new(1);
        let mut state = State::default();
        state.publications.insert(
            "links:important".into(),
            Publication {
                channel: s::ChannelId::new(11),
                message: s::MessageId::new(100),
            },
        );
        store.write(gid, &state).unwrap();
        let plan = Plan {
            operation: 987,
            kind: "links",
            action: "publish".into(),
            key: "links:important".into(),
            document: Document {
                title: "Links".into(),
                ..Default::default()
            },
            channel: s::ChannelId::new(11),
            revision: 0,
            content_revision: Some(0),
            delete: false,
        };
        let (http, server) = mock_discord(vec![(
            status,
            serde_json::json!({"code":code,"message":"Request failed"}).to_string(),
        )]);
        assert!(
            publishing::execute(&store, &catalog, gid, s::UserId::new(9), &http, &plan)
                .await
                .is_err()
        );
        assert_eq!(server.join().unwrap().len(), 1);
        let saved = store.read(gid).unwrap();
        assert_eq!(saved.publications[&plan.key].message.get(), 100);
        assert!(saved.pending.is_none());
        assert!(saved.archived_publications.is_empty());
    }
}

#[tokio::test]
async fn missing_publication_is_preserved_until_confirmed_remove_and_authorship_is_checked() {
    let dir = fixture();
    let config = config::load(dir.path()).unwrap();
    let catalog = Catalog::new(&config);
    let store = Store::open(dir.path().join("data")).unwrap();
    let gid = s::GuildId::new(1);
    let bot = s::UserId::new(9);
    let mut state = State::default();
    state.publications.insert(
        "rules".into(),
        Publication {
            channel: s::ChannelId::new(11),
            message: s::MessageId::new(100),
        },
    );
    store.write(gid, &state).unwrap();
    let mut plan = Plan {
        operation: 1,
        kind: "rules",
        action: "publish".into(),
        key: "rules".into(),
        document: Document {
            title: "Rules".into(),
            ..Default::default()
        },
        channel: s::ChannelId::new(11),
        revision: 0,
        content_revision: Some(0),
        delete: false,
    };
    let missing = r#"{"code":10008,"message":"Unknown Message"}"#.to_string();
    let (http, server) = mock_discord(vec![(404, missing.clone()), (404, missing)]);
    assert!(
        publishing::execute(&store, &catalog, gid, bot, &http, &plan)
            .await
            .is_err()
    );
    assert_eq!(
        store.read(gid).unwrap().publications["rules"].message.get(),
        100
    );
    assert!(store.read(gid).unwrap().pending.is_none());
    plan.delete = true;
    plan.action = "remove".into();
    publishing::execute(&store, &catalog, gid, bot, &http, &plan)
        .await
        .unwrap();
    server.join().unwrap();
    assert!(store.read(gid).unwrap().publications.is_empty());
    store.write(gid, &state).unwrap();
    plan.delete = false;
    let (http, server) = mock_discord(vec![(200, message(999))]);
    assert!(
        publishing::execute(&store, &catalog, gid, bot, &http, &plan)
            .await
            .is_err()
    );
    server.join().unwrap();
    assert_eq!(
        store.read(gid).unwrap().publications["rules"].message.get(),
        100
    );
}

#[test]
fn invite_activity_is_opt_in_per_guild_and_commands_still_gate_the_feature() {
    let dir = fixture();
    write(
        dir.path(),
        "guilds/One/config.toml",
        "[activities.invite]\nenabled=true",
    );
    let config = config::load(dir.path()).unwrap();
    assert!(config.guilds[&s::GuildId::new(1)].activities.invite.enabled);
    assert!(!config.guilds[&s::GuildId::new(2)].activities.invite.enabled);
    assert!(config.guilds[&s::GuildId::new(1)].command_enabled("links", config.global.modules));
    write(
        dir.path(),
        "guilds/One/commands.toml",
        "[commands]\nlinks=false",
    );
    let config = config::load(dir.path()).unwrap();
    assert!(!config.guilds[&s::GuildId::new(1)].command_enabled("links", config.global.modules));
}

#[tokio::test]
async fn v1_to_v2_migration_is_one_patch_and_failures_never_lose_original_publication() {
    use discord_server_bot::services::components_v2::IS_COMPONENTS_V2;
    for fail in [false, true] {
        let dir = fixture();
        let config = config::load(dir.path()).unwrap();
        let catalog = Catalog::new(&config);
        let store = Store::open(dir.path().join("data")).unwrap();
        let guild = s::GuildId::new(1);
        let mut state = State::default();
        state.publications.insert(
            "links:important".into(),
            Publication {
                channel: s::ChannelId::new(11),
                message: s::MessageId::new(100),
            },
        );
        state.documents.insert(
            "unrelated".into(),
            Document {
                title: "Preserve me".into(),
                ..Default::default()
            },
        );
        store.write(guild, &state).unwrap();
        let plan = Plan {
            operation: 777,
            kind: "links",
            action: "publish".into(),
            key: "links:important".into(),
            document: Document {
                title: "Migrated".into(),
                ..Default::default()
            },
            channel: s::ChannelId::new(11),
            revision: 0,
            content_revision: Some(0),
            delete: false,
        };
        let mut migrated: serde_json::Value = serde_json::from_str(&message(9)).unwrap();
        migrated["flags"] = serde_json::json!(IS_COMPONENTS_V2);
        let second = if fail {
            (
                400,
                r#"{"code":50035,"message":"Invalid Form Body"}"#.into(),
            )
        } else {
            (200, migrated.to_string())
        };
        let (http, server) = mock_discord(vec![(200, message(9)), second]);
        let result =
            publishing::execute(&store, &catalog, guild, s::UserId::new(9), &http, &plan).await;
        assert_eq!(result.is_err(), fail);
        let after = store.read(guild).unwrap();
        assert_eq!(after.publications[&plan.key].message.get(), 100);
        assert_eq!(after.documents["unrelated"].title, "Preserve me");
        assert!(after.archived_publications.is_empty());
        assert!(after.pending.is_none());
        if fail {
            assert_eq!(
                after.publication_history.last().unwrap().outcome,
                "http_rejected"
            );
        }
        assert!(
            store
                .read(s::GuildId::new(2))
                .unwrap()
                .publications
                .is_empty()
        );
        let requests = server.join().unwrap();
        assert_eq!(requests.len(), 2);
        assert!(requests[0].0.starts_with("GET "));
        assert!(
            requests[1]
                .0
                .contains("PATCH /api/v10/channels/11/messages/100")
        );
        assert_eq!(requests[1].1["flags"], IS_COMPONENTS_V2);
        assert!(requests[1].1["content"].is_null());
        assert_eq!(requests[1].1["embeds"], serde_json::json!([]));
    }
}

#[tokio::test]
async fn activity_launch_uses_one_initial_callback_and_only_definitive_rejections_allow_private_fallback()
 {
    use discord_server_bot::{core::interactions::Responses, services::links};
    for (status, code, count) in [
        (204, 0, 1),
        (400, 50035, 2),
        (404, 10062, 1),
        (400, 40060, 1),
    ] {
        let body = if status == 204 {
            String::new()
        } else {
            serde_json::json!({"code":code,"message":"test"}).to_string()
        };
        let mut responses = vec![(status, body)];
        if count == 2 {
            responses.push((204, String::new()));
        }
        let (http, server) = mock_discord(responses);
        let tracker = Responses::default();
        let id = s::InteractionId::new(123);
        let result = links::respond_activity(&http, &tracker, id, "test-only", true, true).await;
        assert_eq!(result.is_ok(), status == 204 || count == 2);
        assert!(
            links::respond_activity(&http, &tracker, id, "test-only", true, true)
                .await
                .is_err()
        );
        let requests = server.join().unwrap();
        assert_eq!(requests.len(), count);
        assert_eq!(requests[0].1["type"], 12);
        if count == 2 {
            assert_eq!(requests[1].1["type"], 4);
            assert_eq!(requests[1].1["data"]["flags"], 64);
            assert_eq!(
                requests[1].1["data"]["content"],
                links::ACTIVITY_UNAVAILABLE
            );
        }
    }
    let (http, server) = mock_discord(vec![(204, String::new())]);
    links::respond_activity(
        &http,
        &Responses::default(),
        s::InteractionId::new(124),
        "test-only",
        false,
        true,
    )
    .await
    .unwrap();
    assert_eq!(
        server.join().unwrap()[0].1["data"]["content"],
        links::ACTIVITY_UNAVAILABLE
    );
}
