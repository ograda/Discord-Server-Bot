use discord_server_bot::{
    config::{self, GuildConfig},
    modules::content_commands::AnnouncementForm,
    services::{
        announcements,
        content::Document,
        storage::{State, template},
    },
};
use poise::{Modal, serenity_prelude as s};
use serde_json::{Value, json};
use std::fs;

fn guild(id: u64, name: &str, icon: Option<&str>) -> s::PartialGuild {
    serde_json::from_value(json!({
        "id":id.to_string(), "name":name, "icon":icon, "owner_id":"99", "verification_level":0,
        "default_message_notifications":0, "explicit_content_filter":0, "roles":[],
        "emojis":[], "features":[], "mfa_level":0, "system_channel_flags":0,
        "premium_tier":0, "preferred_locale":"pt-BR", "nsfw_level":0,
        "stickers":[], "premium_progress_bar_enabled":false
    }))
    .unwrap()
}
fn legacy() -> Document {
    Document {
        title: "Aviso".into(),
        description: "Conteúdo\ncom formatação **mantida**.".into(),
        color: 0x9b59b6,
        image: "https://example.org/main.png".into(),
        thumbnail: "not a valid legacy thumbnail".into(),
        footer: "x".repeat(3000),
        ..Default::default()
    }
}
#[test]
fn constructor_uses_each_discord_guild_name_and_icon_without_leaking_identity() {
    let base = legacy();
    let one = guild(
        1,
        "Receita Federal",
        Some("0123456789abcdef0123456789abcdef"),
    );
    let two = guild(
        2,
        "Outra Comunidade",
        Some("abcdef0123456789abcdef0123456789"),
    );
    for current in [&one, &two] {
        let doc = announcements::for_guild(&base, current).unwrap();
        assert_eq!(
            doc.footer,
            format!(
                "Comunicado oficial • {} • Ograda Solutions Bot",
                current.name
            )
        );
        assert_eq!(doc.thumbnail, current.icon_url().unwrap());
        assert_eq!(doc.title, base.title);
        assert_eq!(doc.description, base.description);
        assert_eq!(doc.color, base.color);
        assert_eq!(doc.image, base.image);
        let embeds = serde_json::to_value(doc.embeds("announcements").unwrap()).unwrap();
        assert_eq!(embeds[0]["footer"]["text"], doc.footer);
        assert_eq!(embeds[0]["thumbnail"]["url"], doc.thumbnail);
        assert_eq!(embeds[0]["image"]["url"], base.image);
    }
    assert_eq!(base, legacy());
}
#[test]
fn missing_icon_has_no_thumbnail_and_refresh_replaces_persisted_guild_identity() {
    let mut current = guild(1, "Antigo", Some("0123456789abcdef0123456789abcdef"));
    let previous = announcements::for_guild(&legacy(), &current).unwrap();
    current.name = "Novo nome".into();
    current.icon = None;
    let doc = announcements::for_guild(&previous, &current).unwrap();
    assert_eq!(
        doc.footer,
        "Comunicado oficial • Novo nome • Ograda Solutions Bot"
    );
    assert!(doc.thumbnail.is_empty());
    let embed = serde_json::to_value(doc.embeds("announcements").unwrap()).unwrap();
    assert!(embed[0].get("thumbnail").is_none());
    assert_eq!(doc.image, previous.image);
}
#[test]
fn quick_and_managed_announcement_preparation_share_visual_constructor() {
    let current = guild(1, "Receita Federal", None);
    let quick = announcements::with_message(&legacy(), "Comunicado novo").unwrap();
    let quick = announcements::for_guild(&quick, &current).unwrap();
    let managed = announcements::for_guild(
        &Document {
            description: "Comunicado novo".into(),
            ..legacy()
        },
        &current,
    )
    .unwrap();
    assert_eq!(quick, managed);
    assert!(
        announcements::for_guild(
            &Document {
                description: "x".repeat(4097),
                ..legacy()
            },
            &current
        )
        .is_err()
    );
}
#[test]
fn admin_modal_has_only_title_content_color_and_main_image() {
    let modal: Value = serde_json::to_value(AnnouncementForm::create(None, "test".into())).unwrap();
    let rows = modal["data"]["components"].as_array().unwrap();
    assert_eq!(rows.len(), 4);
    let labels = rows
        .iter()
        .map(|row| row["components"][0]["label"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        labels,
        [
            "Título (opcional)",
            "Mensagem",
            "Cor hexadecimal (#5865F2)",
            "URL alternativa (opcional)"
        ]
    );
    assert_eq!(rows[0]["components"][0]["required"], false);
}

#[test]
fn default_header_uses_the_current_discord_guild_name_and_preserves_identity_and_images() {
    for current in [
        guild(
            1,
            "Receita Federal",
            Some("0123456789abcdef0123456789abcdef"),
        ),
        guild(2, "Comunidade São José", None),
    ] {
        for title in ["", " \n", "📢 Anúncio", "announcements"] {
            for image in [
                "https://example.org/banner.png",
                "attachment://announcement-test.png",
            ] {
                let base = Document {
                    title: title.into(),
                    image: image.into(),
                    ..legacy()
                };
                let rendered = announcements::for_guild(&base, &current).unwrap();
                assert_eq!(
                    rendered.title,
                    format!(
                        "📢 Comunicado oficial da administração da comunidade — {}",
                        current.name
                    )
                );
                assert_eq!(
                    rendered.footer,
                    format!(
                        "Comunicado oficial • {} • Ograda Solutions Bot",
                        current.name
                    )
                );
                assert_eq!(rendered.thumbnail, current.icon_url().unwrap_or_default());
                assert_eq!(rendered.image, image);
                assert_eq!(rendered.color, base.color);
                assert_eq!(rendered.description, base.description);
                assert_eq!(base.title, title);
            }
        }
    }
}
#[test]
fn administrator_custom_headers_are_preserved_verbatim() {
    let current = guild(1, "Receita Federal", None);
    for title in [
        "Aviso importante",
        "📢 Manutenção programada",
        "📢 Comunicado oficial da administração da comunidade — Evento especial",
    ] {
        let base = Document {
            title: title.into(),
            ..legacy()
        };
        assert!(!announcements::uses_default_title(title));
        assert_eq!(
            announcements::for_guild(&base, &current).unwrap().title,
            title
        );
    }
}
#[test]
fn quick_preview_and_publish_share_default_header_without_mutating_the_draft() {
    let current = guild(1, "Receita Federal", None);
    let base = Document {
        title: String::new(),
        ..legacy()
    };
    let quick = announcements::for_guild(
        &announcements::with_message(&base, "Comunicado").unwrap(),
        &current,
    )
    .unwrap();
    let draft = Document {
        description: "Comunicado".into(),
        ..base.clone()
    };
    let preview = announcements::for_guild(&draft, &current).unwrap();
    let publish = announcements::for_guild(&draft, &current).unwrap();
    assert_eq!(quick, preview);
    assert_eq!(preview, publish);
    assert_eq!(
        serde_json::to_value(preview.embeds("announcements").unwrap()).unwrap()[0]["title"],
        "📢 Comunicado oficial da administração da comunidade — Receita Federal"
    );
    assert!(draft.title.is_empty());
    let renamed = guild(1, "Comunidade renomeada", None);
    assert_eq!(
        announcements::for_guild(&draft, &renamed).unwrap().title,
        "📢 Comunicado oficial da administração da comunidade — Comunidade renomeada"
    );
}
#[test]
fn legacy_templates_and_persisted_overrides_ignore_thumbnail_footer_without_rewriting_files() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir_all(dir.path().join("global/announcements")).unwrap();
    let path = dir.path().join("global/announcements/default.toml");
    let original = toml::to_string(&legacy()).unwrap();
    fs::write(&path, &original).unwrap();
    let cfg: GuildConfig = toml::from_str("guild_id='1'").unwrap();
    let mut state = State::default();
    let doc = template(dir.path(), &cfg, "announcements", &state).unwrap();
    assert!(doc.thumbnail.is_empty());
    assert!(doc.footer.is_empty());
    state.documents.insert("announcements".into(), legacy());
    let doc = template(dir.path(), &cfg, "announcements", &state).unwrap();
    assert!(doc.thumbnail.is_empty());
    assert!(doc.footer.is_empty());
    assert_eq!(fs::read_to_string(path).unwrap(), original);
}
#[test]
fn registered_toml_ignores_old_identity_before_validation() {
    let dir = tempfile::tempdir().unwrap();
    let defaults = dir.path().join("guilds/defaults");
    fs::create_dir_all(&defaults).unwrap();
    fs::write(dir.path().join("default.toml"), "app_name='Test'\nlog_level='info'\n[modules]\nadministration=true\nmessaging=true\nroles=false\n[guilds.one]\nid='1'\nname='One'").unwrap();
    fs::write(
        defaults.join("config.toml"),
        "[channels]\nannouncements='10'",
    )
    .unwrap();
    let path = defaults.join("announcements.toml");
    let original = toml::to_string(&legacy()).unwrap();
    fs::write(&path, &original).unwrap();
    let config = config::load(dir.path()).unwrap();
    let doc = &config.guild("one").unwrap().contents["announcements"];
    assert!(doc.thumbnail.is_empty());
    assert!(doc.footer.is_empty());
    assert_eq!(doc.image, legacy().image);
    assert_eq!(fs::read_to_string(path).unwrap(), original);
}
