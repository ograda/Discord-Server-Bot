use discord_server_bot::{
    config,
    services::{
        announcements,
        content::Document,
        profile::{self, MemberProfile, Patch, ProfileConfig, ProfileState},
        storage::{Publication, State, Store},
    },
};
use poise::serenity_prelude as s;
use std::{fs, io::Cursor};

fn png() -> Vec<u8> {
    let image = image::DynamicImage::new_rgba8(2, 2);
    let mut bytes = Cursor::new(Vec::new());
    image.write_to(&mut bytes, image::ImageFormat::Png).unwrap();
    bytes.into_inner()
}

#[test]
fn persisted_profile_patch_is_validated_before_sending() {
    let mut patch = Patch {
        avatar: Some(profile::image_data(&png()).unwrap()),
        ..Default::default()
    };
    assert!(patch.validate().is_ok());
    patch.avatar = Some("data:image/png;base64,bm90IGFuIGltYWdl".into());
    assert!(patch.validate().is_err());
    patch.avatar = None;
    patch.nick = Some(" ".into());
    assert!(patch.validate().is_err());
    patch.nick = None;
    patch.bio = Some("x".repeat(191));
    assert!(patch.validate().is_err());
}
fn state() -> State {
    let mut state = State::default();
    let key = announcements::new_id(123);
    state.documents.insert(
        key.clone(),
        Document {
            title: "Anúncio".into(),
            ..Default::default()
        },
    );
    state.publications.insert(
        key,
        Publication {
            channel: s::ChannelId::new(10),
            message: s::MessageId::new(20),
        },
    );
    state
}
#[test]
fn announcement_identifiers_are_unambiguous_and_guild_scoped() {
    let state = state();
    assert_eq!(announcements::new_id(123), "announcement-123");
    for input in ["announcement-123", "20", " 20 "] {
        assert_eq!(
            announcements::resolve(&state, input).unwrap(),
            "announcement-123"
        );
    }
    for input in ["", "abc", "0", "10", "123", "announcement-999"] {
        assert!(announcements::resolve(&state, input).is_err());
    }
    assert!(announcements::resolve(&State::default(), "20").is_err());
    assert_eq!(
        announcements::summary(&state, "announcement-123"),
        "Anúncio"
    );
    assert_eq!(
        announcements::message_link(s::GuildId::new(1), &state.publications["announcement-123"]),
        "https://discord.com/channels/1/10/20"
    );
}
#[test]
fn announcement_management_survives_restart_preserving_message_id() {
    let dir = tempfile::tempdir().unwrap();
    let guild = s::GuildId::new(1);
    let store = Store::open(dir.path().into()).unwrap();
    store.write(guild, &state()).unwrap();
    drop(store);
    let store = Store::open(dir.path().into()).unwrap();
    let mut state = store.read(guild).unwrap();
    let key = announcements::resolve(&state, "20").unwrap();
    state.documents.get_mut(&key).unwrap().title = "Editado".into();
    store.write(guild, &state).unwrap();
    assert_eq!(
        store.read(guild).unwrap().publications[&key].message.get(),
        20
    );
    state.publications.remove(&key);
    state.documents.remove(&key);
    store.write(guild, &state).unwrap();
    assert!(announcements::resolve(&store.read(guild).unwrap(), &key).is_err());
    assert!(
        store
            .read(s::GuildId::new(2))
            .unwrap()
            .publications
            .is_empty()
    );
}
#[test]
fn incomplete_and_ambiguous_records_are_diagnosed() {
    let mut state = state();
    state.documents.clear();
    assert!(
        announcements::resolve(&state, "20")
            .unwrap_err()
            .to_string()
            .contains("incompleto")
    );
    let mut state = self::state();
    state.publications.insert(
        "announcement-456".into(),
        state.publications["announcement-123"].clone(),
    );
    assert!(announcements::resolve(&state, "20").is_err());
}
#[test]
fn profile_validation_and_optional_legacy_config() {
    for value in ["", " ", "a\n", &"a".repeat(33)] {
        assert!(profile::validate_nickname(value).is_err());
    }
    assert!(profile::validate_nickname("Meu Bot 🎮").is_ok());
    assert!(profile::validate_bio(&"á".repeat(190)).is_ok());
    assert!(profile::validate_bio(&"a".repeat(191)).is_err());
    let guild: config::GuildConfig = toml::from_str("guild_id='1'").unwrap();
    assert!(guild.profile.nickname.is_none());
    let state: State =
        serde_json::from_str(r#"{"revision":0,"documents":{},"publications":{},"pending":null}"#)
            .unwrap();
    assert!(state.profile.nickname.is_none());
}
#[test]
fn avatar_bytes_are_decoded_and_limited() {
    assert!(
        profile::image_data(&png())
            .unwrap()
            .starts_with("data:image/png;base64,")
    );
    for bytes in [&b"not image"[..], &b"\x89PNG\r\n\x1a\n"[..], &[][..]] {
        assert!(profile::image_data(bytes).is_err());
    }
    assert!(profile::image_data(&vec![0; profile::MAX_AVATAR_BYTES + 1]).is_err());
    let cfg = ProfileConfig {
        avatar_data: Some("SECRET_BASE64".into()),
        ..Default::default()
    };
    assert!(!format!("{cfg:?}").contains("SECRET_BASE64"));
}
#[test]
fn assets_are_confined_to_the_guild_directory() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir(dir.path().join("assets")).unwrap();
    fs::write(dir.path().join("assets/avatar.png"), png()).unwrap();
    assert!(profile::read_asset(dir.path(), "assets/avatar.png").is_ok());
    for invalid in [
        "../avatar.png",
        "assets/../avatar.png",
        "/avatar.png",
        "C:/avatar.png",
        "assets\\avatar.png",
        "missing.png",
    ] {
        assert!(profile::read_asset(dir.path(), invalid).is_err());
    }
    let other = tempfile::tempdir().unwrap();
    assert!(profile::read_asset(other.path(), "assets/avatar.png").is_err());
}
#[test]
fn profile_precedence_and_idempotent_apply() {
    let config = ProfileConfig {
        nickname: Some("TOML".into()),
        bio: Some("Default".into()),
        avatar_data: Some("data".into()),
        ..Default::default()
    };
    let saved = ProfileState {
        nickname: Some("Persistido".into()),
        bio: Some("Personalizada".into()),
        applied_bio: Some("Personalizada".into()),
        applied_avatar_digest: Some(profile::digest("data")),
        applied_avatar_hash: Some("hash".into()),
        ..Default::default()
    };
    let desired = profile::effective(&config, &saved);
    assert_eq!(desired.nick.as_deref(), Some("Persistido"));
    let current = MemberProfile {
        nick: desired.nick.clone(),
        bio: desired.bio.clone(),
        avatar: Some("hash".into()),
    };
    assert!(profile::changes(&desired, &current, &saved).is_empty());
    let changed = MemberProfile {
        avatar: Some("outro".into()),
        ..current
    };
    assert!(
        profile::changes(&desired, &changed, &saved)
            .avatar
            .is_some()
    );
    let payload = serde_json::to_value(Patch {
        bio: Some("Texto".into()),
        ..Default::default()
    })
    .unwrap();
    assert_eq!(payload, serde_json::json!({"bio":"Texto"}));
    assert!(payload.get("username").is_none());
}
#[test]
fn profile_state_is_persistent_and_isolated() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().into()).unwrap();
    let mut state = State::default();
    state.profile.nickname = Some("Guilda 1".into());
    state.profile.avatar = Some(profile::image_data(&png()).unwrap());
    store.write(s::GuildId::new(1), &state).unwrap();
    drop(store);
    let store = Store::open(dir.path().into()).unwrap();
    assert_eq!(
        store
            .read(s::GuildId::new(1))
            .unwrap()
            .profile
            .nickname
            .as_deref(),
        Some("Guilda 1")
    );
    assert!(
        store
            .read(s::GuildId::new(2))
            .unwrap()
            .profile
            .avatar
            .is_none()
    );
}
#[test]
fn profile_module_and_rules_links_registration() {
    let global: config::GlobalConfig =
        toml::from_str(include_str!("fixtures/global.toml")).unwrap();
    let mut guild: config::GuildConfig = toml::from_str("guild_id='1'\n[modules]\nadministration=true\nmessaging=true\nroles=true\nrules=true\nlinks=true\nprofile=true\n[profile]\nnickname='Bot'\nbio='Community'").unwrap();
    let commands = config::availability::registered(&guild, global.modules);
    for (name, count) in [("rules", 7), ("links", 7), ("profile", 5)] {
        assert_eq!(
            commands
                .iter()
                .find(|c| c.name == name)
                .unwrap()
                .subcommands
                .len(),
            count
        );
    }
    guild.commands.insert("rules.publish".into(), false);
    assert_eq!(
        config::availability::omission_reason(&guild, "rules.publish", global.modules),
        Some("commands.toml desabilita a entrada")
    );
    guild.modules.as_mut().unwrap().links = false;
    assert!(
        !config::availability::registered(&guild, global.modules)
            .iter()
            .any(|c| c.name == "links")
    );
    assert!(
        config::availability::omission_reason(&guild, "links.preview", global.modules).is_some()
    );
}
