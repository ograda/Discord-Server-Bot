use discord_server_bot::{
    config,
    services::{
        components_v2::{self as v2, IS_COMPONENTS_V2, RemoteMessage},
        content::{Document, Entry},
        links::{self, CopyOrigin},
        storage::{Publication, State},
    },
};
use poise::serenity_prelude as s;
use serde_json::{Value, json};

fn document() -> Document {
    Document {
        title: "🔗 Links da Comunidade".into(),
        description: "Introdução\ncom quebra".into(),
        color: 0x5865f2,
        entries: vec![
            Entry {
                id: "discord".into(),
                title: "Discord".into(),
                url: "https://discord.gg/EXEMPLO".into(),
                emoji: "🎮".into(),
                description: "Compartilhe.".into(),
                ..Default::default()
            },
            Entry {
                id: "whatsapp".into(),
                title: "WhatsApp".into(),
                url: "https://chat.whatsapp.com/EXEMPLO".into(),
                emoji: "📱".into(),
                label: Some("Entrar no grupo".into()),
                description: "Participe.\nOutra linha.".into(),
                ..Default::default()
            },
            Entry {
                id: "site".into(),
                title: "Site".into(),
                url: "https://example.org".into(),
                ..Default::default()
            },
        ],
        ..Default::default()
    }
}
fn remote(flags: u64) -> Value {
    let mut message = s::Message::default();
    message.id = s::MessageId::new(100);
    message.channel_id = s::ChannelId::new(11);
    message.guild_id = Some(s::GuildId::new(1));
    message.author.id = s::UserId::new(9);
    let mut value = serde_json::to_value(message).unwrap();
    value["flags"] = json!(flags);
    value
}
#[test]
fn one_colored_container_markdown_spacing_order_and_nested_buttons_without_embeds() {
    let doc = document();
    let payload = serde_json::to_value(v2::build(&doc, "links-copy:123", vec![]).unwrap()).unwrap();
    assert_eq!(payload["flags"], IS_COMPONENTS_V2);
    assert!(payload.get("content").is_none());
    assert!(payload.get("embeds").is_none());
    assert_eq!(payload["allowed_mentions"]["parse"], json!([]));
    assert_eq!(payload["components"].as_array().unwrap().len(), 1);
    let container = &payload["components"][0];
    assert_eq!(container["type"], 17);
    assert_eq!(container["accent_color"], 0x5865f2);
    let children = container["components"].as_array().unwrap();
    assert_eq!(
        children[0]["content"],
        "## 🔗 Links da Comunidade\n\nIntrodução\ncom quebra"
    );
    let text = children[1]["content"].as_str().unwrap();
    assert!(text.contains("📱 WhatsApp — [Entrar no grupo](<https://chat.whatsapp.com/EXEMPLO>)**\nParticipe.\nOutra linha.\n\n**Site"));
    assert!(text.contains("[Abrir link](<https://example.org>)"));
    assert!(text.ends_with(
        "**🎮 Discord — [Link do convite](<https://discord.gg/EXEMPLO>)**\nCompartilhe."
    ));
    assert_eq!(doc.entries[0].id, "discord"); // TOML order unchanged.
    assert_eq!(children[2]["type"], 14);
    assert_eq!(children[3]["type"], 1);
    let buttons = children[3]["components"].as_array().unwrap();
    assert_eq!(buttons[0]["custom_id"], "links-copy:123");
    assert_eq!(buttons[1]["custom_id"], links::INVITE_FRIENDS_ID);
    assert!(
        buttons
            .iter()
            .all(|b| b.get("url").is_none() && b["style"] == 2)
    );
}
#[test]
fn guild_thumbnail_section_and_optional_media_stay_in_the_same_container() {
    let mut doc = links::with_guild_icon(
        document(),
        Some("https://cdn.discordapp.com/icons/1/icon.png".into()),
    );
    doc.image = "https://example.org/banner.png".into();
    doc.footer = "Rodapé\nexistente".into();
    let payload = serde_json::to_value(v2::build(&doc, "links-copy:1", vec![]).unwrap()).unwrap();
    let children = &payload["components"][0]["components"];
    assert_eq!(children[0]["type"], 9);
    assert_eq!(children[0]["accessory"]["type"], 11);
    assert_eq!(children[0]["accessory"]["media"]["url"], doc.thumbnail);
    assert_eq!(children[2]["type"], 12);
    assert_eq!(children[3]["content"], doc.footer);
    doc = links::with_guild_icon(doc, None);
    let payload = serde_json::to_value(v2::build(&doc, "links-copy:1", vec![]).unwrap()).unwrap();
    assert_eq!(payload["components"][0]["components"][0]["type"], 10);
}
#[test]
fn validation_rejects_oversize_ambiguous_invites_bad_media_and_controls_without_truncation() {
    let mut doc = document();
    doc.description = "😀".repeat(2001);
    assert!(v2::build(&doc, "copy", vec![]).is_err());
    doc = document();
    doc.entries.push(Entry {
        id: "duplicate-invite".into(),
        ..doc.entries[0].clone()
    });
    assert!(v2::build(&doc, "copy", vec![]).is_err());
    doc = document();
    doc.image = "javascript:alert(1)".into();
    assert!(v2::build(&doc, "copy", vec![]).is_err());
    assert!(
        v2::build(
            &document(),
            "copy",
            (0..6)
                .map(|i| s::CreateButton::new(i.to_string()))
                .collect()
        )
        .is_err()
    );
    let layout = v2::build(&document(), "copy", vec![]).unwrap();
    assert!(layout.notice(&"x".repeat(4000)).is_err());
}
#[test]
fn legacy_conversion_clears_only_disallowed_fields_and_preserves_attachment_references() {
    let mut old = remote(4);
    old["content"] = json!("legacy");
    old["embeds"] = json!([{ "title":"old" }]);
    old["attachments"] = json!([{
        "id":"201", "filename":"legacy.png", "size":22,
        "url":"https://cdn.discordapp.com/attachments/1/201/legacy.png",
        "proxy_url":"https://media.discordapp.net/attachments/1/201/legacy.png"
    }]);
    let legacy: RemoteMessage = serde_json::from_value(old).unwrap();
    assert!(!legacy.is_v2());
    let layout = v2::build(&document(), "copy", vec![]).unwrap();
    let payload = layout.edit_payload(&legacy);
    assert_eq!(payload["flags"], IS_COMPONENTS_V2 | 4);
    assert!(payload["content"].is_null() && payload["poll"].is_null());
    assert_eq!(payload["embeds"], json!([]));
    assert!(payload.get("sticker_ids").is_none());
    assert_eq!(
        payload["attachments"],
        json!([{"id":"201","filename":"legacy.png"}])
    );
    assert_eq!(legacy.id.get(), 100);
    let migrated: RemoteMessage = serde_json::from_value(remote(IS_COMPONENTS_V2)).unwrap();
    assert!(migrated.is_v2());
    assert_eq!(layout.edit_payload(&migrated)["flags"], IS_COMPONENTS_V2);
}
#[test]
fn gateway_can_receive_v2_message_with_section_separator_and_nested_button_row() {
    let doc = links::with_guild_icon(
        document(),
        Some("https://cdn.discordapp.com/icons/1/icon.png".into()),
    );
    let payload = serde_json::to_value(v2::build(&doc, "links-copy:123", vec![]).unwrap()).unwrap();
    let mut message = remote(IS_COMPONENTS_V2);
    message["components"] = payload["components"].clone();
    let interaction = json!({
        "id":"123", "application_id":"9", "type":3, "token":"test-only", "version":1,
        "guild_id":"1", "channel_id":"11", "locale":"pt-BR", "entitlements":[], "attachment_size_limit":10485760,
        "data":{"custom_id":links::INVITE_FRIENDS_ID,"component_type":2},
        "user":{"id":"22", "username":"test", "discriminator":"0", "avatar":null},
        "message":message
    });
    let event: s::Interaction = serde_json::from_value(interaction).unwrap();
    let s::Interaction::Component(component) = event else {
        panic!("component expected");
    };
    assert_eq!(component.message.id.get(), 100);
    assert_eq!(component.data.custom_id, links::INVITE_FRIENDS_ID);
    // 0.12.5 deliberately skips unknown top-level components. The interaction
    // and routing metadata survive; handlers must not inspect legacy builders.
    assert!(component.message.components.is_empty());
    assert_eq!(component.guild_id, Some(s::GuildId::new(1)));
    assert_eq!(component.channel_id, s::ChannelId::new(11));
    assert_eq!(component.message.author.id, s::UserId::new(9));
}
#[test]
fn activity_payload_is_type_12_or_private_fallback_with_no_redirect() {
    assert_eq!(
        serde_json::to_value(links::activity_response(true, true)).unwrap()["type"],
        12
    );
    let disabled = serde_json::to_value(links::activity_response(false, true)).unwrap();
    assert_eq!(disabled["type"], 4);
    assert_eq!(disabled["data"]["flags"], 64);
    assert_eq!(disabled["data"]["content"], links::ACTIVITY_UNAVAILABLE);
    let invalid = serde_json::to_value(links::activity_response(true, false)).unwrap();
    assert_eq!(invalid["type"], 4);
    assert_eq!(invalid["data"]["flags"], 64);
}
#[test]
fn invite_config_defaults_to_disabled_and_publication_validation_survives_restart() {
    let disabled: config::Activities = toml::from_str("").unwrap();
    assert!(!disabled.invite.enabled);
    let enabled: config::Activities = toml::from_str("[invite]\nenabled=true").unwrap();
    assert!(enabled.invite.enabled);
    assert!(!disabled.invite.enabled); // Independent guild settings.
    let mut state = State::default();
    state.publications.insert(
        "links:test".into(),
        Publication {
            channel: s::ChannelId::new(11),
            message: s::MessageId::new(100),
        },
    );
    state.link_invites.insert(
        "links:test".into(),
        links::published_invite(&document(), 123).unwrap().unwrap(),
    );
    let state: State = serde_json::from_str(&serde_json::to_string(&state).unwrap()).unwrap();
    let origin = CopyOrigin {
        guild: s::GuildId::new(1),
        message_guild: Some(s::GuildId::new(1)),
        channel: s::ChannelId::new(11),
        message: s::MessageId::new(100),
        author: s::UserId::new(9),
        bot: s::UserId::new(9),
    };
    assert!(links::resolve_activity(&state, origin).is_ok());
    assert!(links::resolve_copy(&state, origin, "links-copy:123").is_ok());
    assert!(
        links::resolve_activity(
            &state,
            CopyOrigin {
                guild: s::GuildId::new(2),
                ..origin
            }
        )
        .is_err()
    );
    assert!(
        links::resolve_activity(
            &state,
            CopyOrigin {
                message: s::MessageId::new(999),
                ..origin
            }
        )
        .is_err()
    );
    assert!(links::resolve_activity(&State::default(), origin).is_err());
}
