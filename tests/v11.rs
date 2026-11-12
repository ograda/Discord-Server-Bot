use discord_server_bot::{
    config::{self, GuildConfig},
    services::{
        content::{Document, Entry, valid_url},
        storage::{State, Store, template},
    },
};
use poise::serenity_prelude as s;
use std::{fs, path::Path, sync::Arc};

fn guild(id: u64, alias: &str) -> GuildConfig {
    toml::from_str(&format!("guild_id = '{id}'\nalias = '{alias}'\n[channels]\nannouncements = '10'\nrules = '11'\nlinks = '12'\n[channels.custom]\nevents = '13'\n")).unwrap()
}
#[test]
fn aliases_and_backward_compatibility() {
    let old: GuildConfig =
        toml::from_str("guild_id = '1'\n[channels]\nannouncements = '10'").unwrap();
    assert!(old.alias.is_none());
    assert!(old.templates.is_empty());
    let g = guild(1, "one");
    assert_eq!(g.channel("events").unwrap().get(), 13);
    assert!(g.channel("missing").is_err());
    assert!(guild(2, "../invalid").validate_registry().is_err());
    let mut g = g;
    g.channels
        .custom
        .insert("rules".into(), s::ChannelId::new(99));
    assert!(g.validate_registry().is_err());
}
#[test]
fn multiguild_registry_rejects_duplicate_aliases() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("default.toml"),
        include_str!("fixtures/global.toml"),
    )
    .unwrap();
    fs::create_dir(dir.path().join("guilds")).unwrap();
    for (id, alias) in [(1, "one"), (2, "two")] {
        fs::write(
            dir.path().join(format!("guilds/{id}.toml")),
            format!("guild_id = '{id}'\nalias = '{alias}'\n[channels]\nannouncements = '10'\n"),
        )
        .unwrap();
    }
    let config = config::load(dir.path()).unwrap();
    assert_eq!(config.guild("two").unwrap().guild_id.get(), 2);
    assert!(config.guild("missing").is_err());
    fs::write(
        dir.path().join("guilds/2.toml"),
        "guild_id = '2'\nalias = 'one'\n[channels]\nannouncements = '10'",
    )
    .unwrap();
    assert!(
        config::load(dir.path())
            .unwrap_err()
            .to_string()
            .contains("alias")
    );
}
#[test]
fn enabled_modules_require_channels() {
    let global: config::GlobalConfig =
        toml::from_str(include_str!("fixtures/global.toml")).unwrap();
    let mut g = guild(1, "one");
    let mut modules = global.modules;
    modules.rules = true;
    modules.links = true;
    g.modules = Some(modules);
    assert!(g.validate(global.modules).is_ok());
    g.channels.links = None;
    assert!(g.validate(global.modules).is_err());
}

#[test]
fn channel_overwrites_and_role_hierarchy_are_enforced() {
    use discord_server_bot::core::permissions::{
        Authority, ensure_bot_channel_permissions, ensure_bot_permissions, higher,
    };
    let mut guild: s::PartialGuild = serde_json::from_value(serde_json::json!({
        "id":"1", "name":"Test", "owner_id":"99", "verification_level":0,
        "default_message_notifications":0, "explicit_content_filter":0, "roles":[],
        "emojis":[], "features":[], "mfa_level":0, "system_channel_flags":0,
        "premium_tier":0, "preferred_locale":"pt-BR", "nsfw_level":0,
        "stickers":[], "premium_progress_bar_enabled":false
    }))
    .unwrap();
    let mut role = s::Role::default();
    role.id = s::RoleId::new(1);
    role.permissions =
        s::Permissions::VIEW_CHANNEL | s::Permissions::SEND_MESSAGES | s::Permissions::EMBED_LINKS;
    guild.roles.insert(role.id, role.clone());
    let mut bot = s::Member::default();
    bot.user.id = s::UserId::new(2);
    bot.guild_id = guild.id;
    let mut high = role.clone();
    high.id = s::RoleId::new(3);
    high.position = 3;
    high.permissions |= s::Permissions::MANAGE_ROLES;
    bot.roles.push(high.id);
    guild.roles.insert(high.id, high.clone());
    let auth = Authority {
        guild,
        actor: s::Member::default(),
        bot,
    };
    let mut channel = s::GuildChannel::default();
    channel.guild_id = s::GuildId::new(1);
    let send = s::Permissions::VIEW_CHANNEL | s::Permissions::SEND_MESSAGES;
    assert!(ensure_bot_channel_permissions(&auth, &channel, send).is_ok());
    assert!(ensure_bot_permissions(&auth, s::Permissions::MANAGE_MESSAGES).is_err());
    assert!(ensure_bot_permissions(&auth, s::Permissions::MANAGE_ROLES).is_ok());
    assert!(higher(&auth.guild, &auth.bot, &role));
    assert!(!higher(&auth.guild, &auth.bot, &high));
    channel.permission_overwrites.push(s::PermissionOverwrite {
        allow: s::Permissions::empty(),
        deny: s::Permissions::SEND_MESSAGES,
        kind: s::PermissionOverwriteType::Member(auth.bot.user.id),
    });
    assert!(ensure_bot_channel_permissions(&auth, &channel, send).is_err());
    channel.permission_overwrites[0].deny = s::Permissions::EMBED_LINKS;
    assert!(ensure_bot_channel_permissions(&auth, &channel, send).is_ok());
    assert!(
        ensure_bot_channel_permissions(&auth, &channel, send | s::Permissions::EMBED_LINKS)
            .is_err()
    );
    channel.guild_id = s::GuildId::new(9);
    assert!(ensure_bot_channel_permissions(&auth, &channel, send).is_err());
}
#[test]
fn rules_have_one_purple_embed_with_all_ten_rules() {
    let doc: Document =
        toml::from_str(include_str!("../templates/global/rules/default.toml")).unwrap();
    let embeds = doc.embeds("rules").unwrap();
    let json = serde_json::to_value(embeds).unwrap();
    assert_eq!(json.as_array().unwrap().len(), 1);
    for embed in json.as_array().unwrap() {
        assert_eq!(embed["color"], 0x5865f2);
        assert!(
            embed
                .get("fields")
                .is_none_or(|v| v.as_array().unwrap().is_empty())
        );
        assert!(embed.get("footer").is_none());
    }
    assert_eq!(json[0]["title"], doc.title);
    assert!(
        json[0]["description"]
            .as_str()
            .unwrap()
            .contains("Bem-vindo")
    );
    assert!(
        json[0]["description"]
            .as_str()
            .unwrap()
            .contains("moderação")
    );
    for entry in &doc.entries {
        assert!(
            json[0]["description"]
                .as_str()
                .unwrap()
                .contains(&entry.title)
        );
        assert!(
            json[0]["description"]
                .as_str()
                .unwrap()
                .contains(&entry.description)
        );
    }
}
#[test]
fn embed_limits_and_variable_rule_count() {
    let entry = Entry {
        id: "1".into(),
        title: "Regra".into(),
        description: "Texto".into(),
        ..Default::default()
    };
    let mut doc = Document {
        title: "Regras".into(),
        entries: (0..11)
            .map(|i| Entry {
                id: i.to_string(),
                order: i,
                ..entry.clone()
            })
            .collect(),
        ..Default::default()
    };
    assert_eq!(doc.embeds("rules").unwrap().len(), 1);
    doc.entries[0].description = "x".repeat(1025);
    assert!(doc.embeds("rules").is_ok()); // No per-field limit: Rules now uses description.
    doc.entries[0].description = "x".repeat(1000);
    for e in &mut doc.entries {
        e.description = "x".repeat(1000);
    }
    assert!(doc.embeds("rules").is_err());
    doc.entries.clear();
    doc.text = "😀".repeat(1001);
    assert!(doc.embeds("rules").is_err());
}
#[test]
fn rules_single_embed_limit_does_not_change_links_pagination() {
    let mut doc = Document {
        title: "Coleção".into(),
        entries: (0..25)
            .map(|n| Entry {
                id: format!("entry-{n}"),
                title: format!("Item {n}"),
                description: "Descrição".into(),
                url: "https://example.org".into(),
                order: n,
                ..Default::default()
            })
            .collect(),
        ..Default::default()
    };
    let rules = serde_json::to_value(doc.embeds("rules").unwrap()).unwrap();
    assert_eq!(rules.as_array().unwrap().len(), 1);
    assert!(
        rules[0]["description"]
            .as_str()
            .unwrap()
            .contains("**25. Item 24**\nDescrição")
    );
    assert_eq!(doc.embeds("links").unwrap().len(), 5);
    doc.entries.push(Entry {
        id: "entry-25".into(),
        title: "Item 25".into(),
        order: 25,
        ..doc.entries[0].clone()
    });
    let rules = serde_json::to_value(doc.embeds("rules").unwrap()).unwrap();
    assert_eq!(rules.as_array().unwrap().len(), 1);
    assert!(
        rules[0]["description"]
            .as_str()
            .unwrap()
            .contains("**26. Item 25**\nDescrição")
    );
    assert_eq!(doc.embeds("links").unwrap().len(), 6);
}
#[test]
fn rules_description_controls_spacing_preserves_internal_breaks_and_sorted_content() {
    let doc = Document {
        title: "📜 Regras".into(),
        color: 0x5865f2,
        description: "\nIntrodução linha 1\n\nIntrodução linha 2\n".into(),
        footer: "\nEncerramento linha 1\nEncerramento linha 2\n".into(),
        entries: vec![
            Entry {
                id: "second".into(),
                title: "Segunda".into(),
                description: "\nTexto 2\r\nLinha seguinte\n".into(),
                order: 2,
                ..Default::default()
            },
            Entry {
                id: "first".into(),
                title: "Primeira".into(),
                emoji: "🛡️".into(),
                description: "\nTexto 1\n\nDetalhe interno\n".into(),
                order: 1,
                ..Default::default()
            },
        ],
        ..Default::default()
    };
    let original = doc.clone();
    let rendered = serde_json::to_value(doc.embeds("rules").unwrap()).unwrap();
    assert_eq!(rendered.as_array().unwrap().len(), 1);
    assert_eq!(rendered[0]["title"], doc.title);
    assert_eq!(rendered[0]["color"], 0x5865f2);
    assert_eq!(
        rendered[0]["description"],
        "Introdução linha 1\n\nIntrodução linha 2\n\n**1️⃣ 🛡️ Primeira**\nTexto 1\n\nDetalhe interno\n\n**2️⃣ Segunda**\nTexto 2\r\nLinha seguinte\n\nEncerramento linha 1\nEncerramento linha 2"
    );
    assert!(rendered[0].get("footer").is_none());
    assert!(
        rendered[0]
            .get("fields")
            .is_none_or(|v| v.as_array().unwrap().is_empty())
    );
    assert_eq!(doc, original); // Rendering must not mutate TOML-derived content.
}

#[test]
fn rules_validate_complete_description_without_native_footer_limit_or_truncation() {
    let mut doc = Document {
        title: "Regras".into(),
        description: "a".repeat(4096),
        ..Default::default()
    };
    let rendered = serde_json::to_value(doc.embeds("rules").unwrap()).unwrap();
    assert_eq!(rendered[0]["description"].as_str().unwrap().len(), 4096);
    assert!(
        doc.title.encode_utf16().count()
            + rendered[0]["description"]
                .as_str()
                .unwrap()
                .encode_utf16()
                .count()
            <= 6000
    );
    doc.footer = "Fim".into();
    assert!(
        doc.embeds("rules")
            .unwrap_err()
            .to_string()
            .contains("4096")
    );
    assert_eq!(doc.description.len(), 4096);
    doc.description.clear();
    doc.footer = "a".repeat(2049);
    assert!(doc.embeds("rules").is_ok());
    assert!(doc.embeds("links").is_err()); // Other modules retain native footer validation.
    doc.footer.clear();
    doc.description = "😀".repeat(2048);
    assert!(doc.embeds("rules").is_ok());
    doc.description.push('a');
    assert!(doc.embeds("rules").is_err());
    doc.description.clear();
    doc.entries = vec![Entry {
        id: "rule".into(),
        title: "Título".into(),
        description: "a".repeat(4080),
        ..Default::default()
    }];
    doc.footer = "Encerramento".into();
    assert!(
        doc.embeds("rules")
            .unwrap_err()
            .to_string()
            .contains("4096")
    );
}
#[test]
fn validates_links_and_suppresses_mentions() {
    for url in [
        "",
        "javascript:alert(1)",
        "ftp://example.org",
        "https://",
        "https://user:pass@example.org",
    ] {
        assert!(valid_url(url).is_err());
    }
    assert!(valid_url("https://example.org/path?q=1").is_ok());
    let doc = Document {
        title: "Links".into(),
        entries: vec![Entry {
            id: "site".into(),
            title: "Site".into(),
            description: "Descrição".into(),
            url: "https://example.org".into(),
            ..Default::default()
        }],
        ..Default::default()
    };
    assert_eq!(doc.embeds("links").unwrap().len(), 1);
    let mentions = serde_json::to_value(s::CreateAllowedMentions::new()).unwrap();
    assert_eq!(mentions["parse"], serde_json::json!([]));
    let mut invalid = doc.clone();
    invalid.entries.push(doc.entries[0].clone());
    assert!(invalid.embeds("links").is_err());
    let serialized = serde_json::to_string(&doc).unwrap();
    assert_eq!(
        serde_json::from_str::<Document>(&serialized)
            .unwrap()
            .entries[0]
            .id,
        "site"
    );
}

#[test]
fn slash_groups_expose_all_expected_operations() {
    use discord_server_bot::core::commands;
    let commands = commands::all();
    assert_eq!(commands.len(), 8);
    for command in commands {
        if command.name == "role" || command.name == "role-all" {
            assert!(command.subcommands.is_empty());
            assert!(command.parameters.iter().all(|p| p.required));
        } else {
            assert!(command.subcommands.is_empty());
            assert!(command.ephemeral);
            if command.name != "announce" {
                assert!(
                    command
                        .parameters
                        .iter()
                        .any(|p| !p.required && !p.choices.is_empty())
                );
            }
        }
    }
}
#[test]
fn publication_records_survive_atomic_replacement() {
    use discord_server_bot::services::storage::Publication;
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().into()).unwrap();
    let guild = s::GuildId::new(1);
    let mut state = State::default();
    state.publications.insert(
        "rules".into(),
        Publication {
            channel: s::ChannelId::new(10),
            message: s::MessageId::new(20),
        },
    );
    state.pending = Some("rules:operation".into());
    store.write(guild, &state).unwrap();
    state.revision = 1;
    state.pending = None;
    store.write(guild, &state).unwrap();
    drop(store);
    let restored = Store::open(dir.path().into()).unwrap().read(guild).unwrap();
    assert_eq!(restored.publications["rules"].message.get(), 20);
    assert_eq!(restored.revision, 1);
    assert!(restored.pending.is_none());
}
#[test]
fn template_priority_partial_overrides_and_isolation() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir_all(dir.path().join("global/rules")).unwrap();
    fs::create_dir_all(dir.path().join("guilds/1/rules")).unwrap();
    fs::write(
        dir.path().join("global/rules/default.toml"),
        "title = 'Global'\ndescription = 'Shared'\ncolor = 1",
    )
    .unwrap();
    fs::write(
        dir.path().join("guilds/1/rules/default.toml"),
        "title = 'One'",
    )
    .unwrap();
    let mut state = State::default();
    let one = template(dir.path(), &guild(1, "one"), "rules", &state).unwrap();
    assert_eq!(one.title, "One");
    assert_eq!(one.description, "Shared");
    assert_eq!(
        template(dir.path(), &guild(2, "two"), "rules", &state)
            .unwrap()
            .title,
        "Global"
    );
    state.documents.insert(
        "rules".into(),
        Document {
            title: "Persistido".into(),
            ..Default::default()
        },
    );
    assert_eq!(
        template(dir.path(), &guild(1, "one"), "rules", &state)
            .unwrap()
            .title,
        "One" // TOML is authoritative; legacy JSON is archived at startup.
    );
    assert!(
        template(
            Path::new("templates"),
            &guild(1, "one"),
            "rules",
            &State::default()
        )
        .unwrap()
        .entries
        .len()
            == 10
    );
}
#[tokio::test]
async fn storage_restart_concurrency_and_isolation() {
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(Store::open(dir.path().into()).unwrap());
    assert!(Store::open(dir.path().into()).is_err());
    let mut tasks = vec![];
    for _ in 0..20 {
        let store = store.clone();
        tasks.push(tokio::spawn(async move {
            let _guard = store.gate.lock().await;
            let mut state = store.read(s::GuildId::new(1)).unwrap();
            state.revision += 1;
            store.write(s::GuildId::new(1), &state).unwrap();
        }));
    }
    for task in tasks {
        task.await.unwrap();
    }
    assert_eq!(store.read(s::GuildId::new(1)).unwrap().revision, 20);
    assert_eq!(store.read(s::GuildId::new(2)).unwrap().revision, 0);
    drop(store);
    let store = Store::open(dir.path().into()).unwrap();
    assert_eq!(store.read(s::GuildId::new(1)).unwrap().revision, 20);
    fs::write(dir.path().join("guilds/1/state.json"), "broken").unwrap();
    assert!(store.read(s::GuildId::new(1)).is_err());
}
