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
                assert_eq!(choice["required"], false);
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
        ("links", "preview"),
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
        (
            "links",
            "add",
            vec!["quadro", "id", "titulo", "url", "descricao", "emoji"],
        ),
        ("links", "publish", vec!["quadro"]),
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
                        break (
                            header,
                            if len == 0 {
                                serde_json::Value::Null
                            } else {
                                serde_json::from_slice(&request[i + 4..i + 4 + len]).unwrap()
                            },
                        );
                    }
                }
            };
            records.push((headers.lines().next().unwrap().to_string(), payload));
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
        let (http, server) = mock_discord(vec![
            (200, message(9)),
            (200, message(9)),
            (200, message(9)),
            (200, message(9)),
            (204, String::new()),
        ]);
        let gid = s::GuildId::new(1);
        let bot = s::UserId::new(9);
        publishing::execute(&store, &catalog, gid, bot, &http, &plan)
            .await
            .unwrap();
        let state = store.read(gid).unwrap();
        assert_eq!(state.publications[key].message.get(), 100);
        assert_eq!(state.documents.contains_key(key), kind == "announcements");
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
        plan.revision = store.read(gid).unwrap().revision;
        plan.action = "remove".into();
        plan.delete = true;
        publishing::execute(&store, &catalog, gid, bot, &http, &plan)
            .await
            .unwrap();
        let records = server.join().unwrap();
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
        assert_eq!(records[2].1["embeds"][0]["title"], "Updated");
        if kind == "rules" {
            for request in [&records[0], &records[2]] {
                assert_eq!(request.1["embeds"].as_array().unwrap().len(), 1);
                assert_eq!(
                    request.1["embeds"][0]["fields"].as_array().unwrap().len(),
                    10
                );
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
