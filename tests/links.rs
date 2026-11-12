use discord_server_bot::{
    config::{self, availability},
    core::{commands, permissions},
    services::{
        catalog::{Board, LinksFile},
        content::{Document, Entry, LinkKind, discord_invite_url},
        links::{self, CopyOrigin},
        storage::{Publication, State, Store},
    },
};
use poise::serenity_prelude as s;
use serde_json::{Value, json};

fn entry(id: &str, url: &str) -> Entry {
    Entry {
        id: id.into(),
        title: id.into(),
        url: url.into(),
        ..Default::default()
    }
}
fn document() -> Document {
    Document {
        title: "🔗 Links da Comunidade".into(),
        color: 0x9b59b6,
        description: "Introdução\ncom quebra interna".into(),
        entries: vec![
            Entry {
                title: "Convite do Discord".into(),
                emoji: "🎮".into(),
                description: "Compartilhe.".into(),
                ..entry("discord", "https://discord.gg/EXEMPLO")
            },
            Entry {
                title: "Grupo do WhatsApp".into(),
                emoji: "📱".into(),
                label: Some("Entrar no grupo".into()),
                description: "Participe.\nOutra linha.".into(),
                ..entry("whatsapp", "https://chat.whatsapp.com/EXEMPLO")
            },
            entry("site", "http://example.org"),
        ],
        ..Default::default()
    }
}
#[test]
fn schema_has_required_string_choices_and_only_optional_link_id() {
    let command = commands::all()
        .into_iter()
        .find(|c| c.name == "links")
        .unwrap();
    let payload = serde_json::to_value(command.create_as_slash_command().unwrap()).unwrap();
    let options = payload["options"].as_array().unwrap();
    assert_eq!(options.len(), 2);
    assert_eq!(options[0]["name"], "ação");
    assert_eq!(options[0]["type"], 3);
    assert_eq!(options[0]["required"], true);
    let choices = options[0]["choices"].as_array().unwrap();
    assert_eq!(
        choices
            .iter()
            .map(|v| v["value"].as_str().unwrap())
            .collect::<Vec<_>>(),
        links::ACTIONS
    );
    assert_eq!(options[1]["name"], "id");
    assert_eq!(options[1]["type"], 3);
    assert_eq!(options[1]["required"], false);
    assert_eq!(options[1]["autocomplete"], true);
    assert_eq!(availability::default_action("links"), None);
    for action in links::ACTIONS {
        assert!(permissions::conditional_parameters("links", action, &["id"]).is_ok());
        assert_eq!(
            links::removal_id(action, Some("site")),
            (*action == "remove").then_some("site")
        );
    }
    let mut guild: config::GuildConfig = toml::from_str("guild_id='1'").unwrap();
    let global: config::GlobalConfig =
        toml::from_str(include_str!("fixtures/global.toml")).unwrap();
    let mut modules = global.modules;
    modules.administration = true;
    modules.links = true;
    assert!(guild.select_action("links", None, modules).is_err());
    guild
        .actions
        .insert("links".into(), [("publish".into(), false)].into());
    let remote = serde_json::to_value(availability::remote_commands(&guild, modules)).unwrap();
    let links = remote
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == "links")
        .unwrap();
    assert_eq!(links["options"][0]["type"], 3);
    assert_eq!(links["options"][0]["required"], true);
    assert!(
        !links["options"][0]["choices"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c["value"] == "publish")
    );
    assert!(
        guild
            .select_action("links", Some("publish"), modules)
            .is_err()
    );
}
#[test]
fn single_description_preserves_style_spacing_labels_markdown_and_invite_last() {
    let doc = document();
    let before = doc.clone();
    let embeds = serde_json::to_value(doc.embeds("links").unwrap()).unwrap();
    assert_eq!(embeds.as_array().unwrap().len(), 1);
    let embed = &embeds[0];
    assert!(embed.get("fields").is_none());
    assert_eq!(embed["color"], doc.color);
    assert_eq!(embed["title"], doc.title);
    assert_eq!(
        embed["description"],
        "Introdução\ncom quebra interna\n\n**📱 Grupo do WhatsApp — [Entrar no grupo](<https://chat.whatsapp.com/EXEMPLO>)**\nParticipe.\nOutra linha.\n\n**site — [Abrir link](<http://example.org>)**\n\n**🎮 Convite do Discord — [Link do convite](<https://discord.gg/EXEMPLO>)**\nCompartilhe."
    );
    assert_eq!(doc, before); // No TOML or configured order mutation.
    let icon = "https://cdn.discordapp.com/icons/1/icon.png";
    let with_icon = links::with_guild_icon(doc.clone(), Some(icon.into()));
    assert_eq!(
        serde_json::to_value(with_icon.embeds("links").unwrap()).unwrap()[0]["thumbnail"]["url"],
        icon
    );
    assert!(
        serde_json::to_value(links::with_guild_icon(doc, None).embeds("links").unwrap()).unwrap()
            [0]
        .get("thumbnail")
        .is_none()
    );
}
#[test]
fn optional_label_and_kind_are_backward_compatible_and_strictly_validated() {
    let old: Entry = toml::from_str("id='site'\ntitle='Site'\nurl='https://example.org'").unwrap();
    assert_eq!(old.label, None);
    assert_eq!(old.kind, None);
    let legacy = Document {
        title: "Legacy".into(),
        entries: vec![Entry {
            title: "x".repeat(220),
            ..old.clone()
        }],
        ..Default::default()
    };
    assert!(legacy.embeds("links").is_ok()); // Modal limits must not invalidate existing TOMLs.
    let mut doc = document();
    doc.entries[0].kind = Some(LinkKind::DiscordInvite);
    assert!(doc.embeds("links").is_ok());
    doc.entries[0].url = "https://discord.com/channels/1/2".into();
    assert!(
        doc.embeds("links")
            .unwrap_err()
            .to_string()
            .contains("kind=discord_invite")
    );
    for label in ["", " ", "bad\nlabel"] {
        let mut e = old.clone();
        e.label = Some(label.into());
        assert!(e.validate_link().is_err());
    }
    assert!(toml::from_str::<Entry>("id='x'\nkind='unknown'").is_err());
}
#[test]
fn invitation_detection_matches_only_official_invite_addresses() {
    for url in [
        "https://discord.gg/EXEMPLO",
        "https://discord.gg/abc-_123/",
        "http://discord.com/invite/abc",
        "https://DISCORD.GG/abc",
        "https://discord.gg/abc?utm_source=community",
    ] {
        assert!(discord_invite_url(url), "{url}");
    }
    for url in [
        "https://discord.com",
        "https://discord.com/channels/1/2",
        "https://discord.com/invite",
        "https://discord.gg/",
        "https://discord.gg.evil.test/abc",
        "https://evil.test/discord.gg/abc",
        "https://discord.gg//abc",
        "https://discord.gg/abc/other",
        "https://discord.com/invite/abc/other",
        "ftp://discord.gg/abc",
        "https://token@discord.gg/abc",
        "https://discord.gg/abc`",
        "https://discord.gg/abc?code=```",
    ] {
        assert!(!discord_invite_url(url), "{url}");
    }
    let mut doc = document();
    doc.entries
        .push(entry("another", "https://discord.com/invite/OUTRO"));
    assert!(
        doc.embeds("links")
            .unwrap_err()
            .to_string()
            .contains("mais de um convite")
    );
}
#[test]
fn embed_limits_fail_without_truncation_or_pagination() {
    let mut doc = document();
    doc.description = "x".repeat(4096);
    assert!(doc.embeds("links").is_err());
    doc.entries.clear();
    assert_eq!(
        serde_json::to_value(doc.embeds("links").unwrap()).unwrap()[0]["description"]
            .as_str()
            .unwrap()
            .len(),
        4096
    );
    doc.footer = "x".repeat(2048);
    assert!(
        doc.embeds("links")
            .unwrap_err()
            .to_string()
            .contains("6000")
    );
    doc.description = "😀".repeat(2049);
    assert!(doc.embeds("links").is_err());
}
#[test]
fn invitation_components_and_private_response_use_configured_url_without_clipboard_claims() {
    let doc = document();
    let rows =
        serde_json::to_value(links::invite_components(&doc, "links-copy:1").unwrap()).unwrap();
    let buttons = rows[0]["components"].as_array().unwrap();
    assert_eq!(buttons.len(), 2);
    assert_eq!(buttons[0]["custom_id"], "links-copy:1");
    assert_eq!(buttons[0]["label"], "📋 Copiar convite");
    assert!(buttons[0].get("url").is_none());
    assert_eq!(buttons[1]["style"], 2);
    assert_eq!(buttons[1]["custom_id"], links::INVITE_FRIENDS_ID);
    assert_eq!(buttons[1]["label"], "🎮 Convidar amigos");
    assert!(buttons[1].get("url").is_none());
    let response =
        serde_json::to_value(links::copy_response(&doc.entries[0].url).unwrap()).unwrap();
    assert_eq!(response["flags"], 64);
    let text = response["content"].as_str().unwrap();
    assert!(text.contains("```\nhttps://discord.gg/EXEMPLO\n```"));
    assert!(!text.contains("copiado"));
    assert_eq!(response["allowed_mentions"]["parse"], json!([]));
    assert!(links::copy_content("https://evil.test/invite/abc").is_err());
    let mut doc = doc;
    doc.entries.remove(0);
    assert!(links::invite_components(&doc, "unused").unwrap().is_empty());
}
#[test]
fn invite_friends_routes_to_exact_ephemeral_response_without_session_or_permissions() {
    let custom_id = "links-invite-friends";
    assert!(links::handles_invite_component(custom_id));
    let response =
        serde_json::to_value(links::invite_friends_response(custom_id).unwrap()).unwrap();
    assert_eq!(response["flags"], 64);
    assert_eq!(
        response["content"],
        "Lembra o besta do Ograda que essa funcionalidade ainda não está disponível"
    );
    assert_eq!(response["allowed_mentions"]["parse"], json!([]));
    assert!(
        response
            .get("components")
            .is_none_or(|v| v.as_array().unwrap().is_empty())
    );
    assert!(links::invite_friends_response("links-copy:123").is_none());
    assert!(links::handles_invite_component("links-copy:123"));
    assert!(!links::handles_invite_component("unrelated"));
    // Identical across publications and restarts; no collector or transient state is needed.
    for operation in [123, 456] {
        let rows = serde_json::to_value(
            links::invite_components(&document(), &format!("links-copy:{operation}")).unwrap(),
        )
        .unwrap();
        assert_eq!(rows[0]["components"][1]["custom_id"], custom_id);
        assert!(rows[0]["components"][1].get("url").is_none());
    }
}
#[test]
fn persistent_copy_button_checks_publication_origin_and_survives_restart() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().join("data")).unwrap();
    let mut state = State::default();
    let origin = CopyOrigin {
        guild: s::GuildId::new(1),
        message_guild: Some(s::GuildId::new(1)),
        channel: s::ChannelId::new(2),
        message: s::MessageId::new(3),
        author: s::UserId::new(4),
        bot: s::UserId::new(4),
    };
    state.publications.insert(
        "links".into(),
        Publication {
            channel: origin.channel,
            message: origin.message,
        },
    );
    state.link_invites.insert(
        "links".into(),
        links::published_invite(&document(), 123).unwrap().unwrap(),
    );
    store.write(origin.guild, &state).unwrap();
    drop(store);
    let reopened = Store::open(dir.path().join("data")).unwrap();
    let state = reopened.read(origin.guild).unwrap();
    assert_eq!(
        links::resolve_copy(&state, origin, "links-copy:123").unwrap(),
        "https://discord.gg/EXEMPLO"
    );
    for invalid in [
        CopyOrigin {
            message_guild: Some(s::GuildId::new(5)),
            ..origin
        },
        CopyOrigin {
            message: s::MessageId::new(5),
            ..origin
        },
        CopyOrigin {
            channel: s::ChannelId::new(5),
            ..origin
        },
        CopyOrigin {
            author: s::UserId::new(5),
            ..origin
        },
    ] {
        assert!(links::resolve_copy(&state, invalid, "links-copy:123").is_err());
    }
    assert!(links::resolve_copy(&state, origin, "links-copy:stale").is_err());
    let other = reopened.read(s::GuildId::new(2)).unwrap();
    assert!(links::resolve_copy(&other, origin, "links-copy:123").is_err());
}
#[test]
fn board_selection_and_link_autocomplete_resolve_duplicate_ids_unambiguously() {
    let mut file = LinksFile {
        boards: vec![
            Board {
                id: "a".into(),
                links: vec![entry("site", "https://example.org")],
                ..Default::default()
            },
            Board {
                id: "b".into(),
                links: vec![entry("site", "https://example.com")],
                ..Default::default()
            },
        ],
    };
    assert!(links::preferred_board(&file.boards).is_none());
    assert_eq!(links::matching_links(&file, Some("site")).len(), 2);
    let token = links::link_token("b", "site");
    assert!(token.len() <= 100);
    assert_eq!(links::matching_links(&file, Some(&token))[0].0.id, "b");
    let choices: Value = serde_json::to_value(links::autocomplete(&file, "site")).unwrap();
    assert_eq!(choices.as_array().unwrap().len(), 2);
    file.boards[1].id = "default".into();
    assert_eq!(links::preferred_board(&file.boards).unwrap().id, "default");
    file.boards.remove(1);
    assert_eq!(links::preferred_board(&file.boards).unwrap().id, "a");
}
