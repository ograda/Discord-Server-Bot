use discord_server_bot::{
    core::{commands, permissions},
    services::{
        announcements::{
            self,
            media::{self, ImageAction, ImageChange, PublishedImage},
        },
        content::Document,
        storage::{State, Store},
    },
};
use poise::serenity_prelude as s;
use serde_json::json;
use std::io::{Cursor, Read, Write};

fn encoded(format: image::ImageFormat, size: u32) -> Vec<u8> {
    let mut out = Cursor::new(Vec::new());
    image::DynamicImage::new_rgb8(size, size)
        .write_to(&mut out, format)
        .unwrap();
    out.into_inner()
}
fn attachment(id: u64, filename: &str) -> s::Attachment {
    serde_json::from_value(json!({"id":id.to_string(),"filename":filename,"size":1024,"content_type":"image/png", "url":format!("https://cdn.discordapp.com/attachments/11/100/{filename}?ex=fresh"), "proxy_url":format!("https://media.discordapp.net/attachments/11/100/{filename}")})).unwrap()
}
fn original() -> s::Message {
    let mut message = s::Message::default();
    message.id = s::MessageId::new(100);
    message.attachments = vec![attachment(50, "original.png"), attachment(51, "other.txt")];
    message
}
fn original_doc() -> Document {
    Document {
        title: "Aviso".into(),
        description: "Texto editável".into(),
        image: "attachment://original.png".into(),
        ..Default::default()
    }
}
fn metadata() -> PublishedImage {
    PublishedImage::Attachment {
        id: s::AttachmentId::new(50),
        filename: "original.png".into(),
    }
}
#[test]
fn validates_all_formats_preserving_bytes_and_1254_resolution() {
    for (format, ext, mime) in [
        (image::ImageFormat::Png, "png", "image/png"),
        (image::ImageFormat::Jpeg, "jpeg", "image/jpeg"),
        (image::ImageFormat::WebP, "webp", "image/webp"),
        (image::ImageFormat::Gif, "gif", "image/gif"),
    ] {
        let bytes = encoded(format, if ext == "png" { 1254 } else { 32 });
        let upload = media::validate(
            &bytes,
            &format!("upload.{ext}"),
            Some(mime),
            media::MAX_UPLOAD_BYTES,
        )
        .unwrap();
        assert_eq!(upload.attachment().data, bytes);
        assert!(upload.image_url().starts_with("attachment://announcement-"));
        let mut doc = original_doc();
        ImageChange::Upload(upload).apply(&mut doc).unwrap();
        assert_eq!(doc.embeds("announcements").unwrap().len(), 1);
        assert!(doc.embeds("rules").is_err());
    }
}
#[test]
fn rejects_invalid_bytes_mime_extension_size_and_malformed_images() {
    let png = encoded(image::ImageFormat::Png, 8);
    for (bytes, name, mime, limit) in [
        (b"not an image".as_slice(), "file.png", "image/png", 1024),
        (&[], "empty.png", "image/png", 1024),
        (&png, "file.jpg", "image/png", 1024),
        (&png, "file.png", "image/jpeg", 1024),
        (&png, "file.exe", "image/png", 1024),
        (&png, "file.png", "application/octet-stream", 1024),
        (&png, "file.png", "image/png", png.len() - 1),
        (&png[..png.len() / 2], "file.png", "image/png", 1024),
    ] {
        assert!(media::validate(bytes, name, Some(mime), limit).is_err());
    }
    assert!(
        media::validate(
            &vec![0; media::MAX_UPLOAD_BYTES + 1],
            "file.png",
            None,
            usize::MAX
        )
        .is_err()
    );
}
#[tokio::test]
async fn absent_upload_keeps_image_and_conflicting_sources_or_actions_fail() {
    assert!(matches!(
        media::input(None, None, None, 1024).await.unwrap(),
        ImageChange::Keep
    ));
    assert!(matches!(
        media::input(Some(ImageAction::Remove), None, None, 1024)
            .await
            .unwrap(),
        ImageChange::Remove
    ));
    assert!(matches!(
        media::input(None, None, Some("https://example.org/banner.png"), 1024)
            .await
            .unwrap(),
        ImageChange::External(_)
    ));
    assert!(
        media::input(
            None,
            Some(&attachment(1, "file.png")),
            Some("https://example.org/banner.png"),
            1024
        )
        .await
        .is_err()
    );
    for action in [ImageAction::Keep, ImageAction::Remove] {
        assert!(
            media::input(
                Some(action),
                None,
                Some("https://example.org/banner.png"),
                1024
            )
            .await
            .is_err()
        );
    }
    assert!(
        media::input(Some(ImageAction::Replace), None, None, 1024)
            .await
            .is_err()
    );
    assert!(
        media::input(None, None, Some("file:///private.png"), 1024)
            .await
            .is_err()
    );
    let mut invalid = attachment(1, "file.png");
    invalid.url = "http://127.0.0.1/private".into();
    assert!(media::download(&invalid, 1024).await.is_err());
}
#[test]
fn text_edit_explicitly_keeps_image_and_unrelated_attachments() {
    let message = original();
    let mut doc = original_doc();
    doc.description = "Novo texto".into();
    let keep =
        media::edit_attachments(&mut doc, &ImageChange::Keep, &message, Some(&metadata())).unwrap();
    assert_eq!(
        serde_json::to_value(keep).unwrap(),
        json!([{"id":"50"},{"id":"51"}])
    );
    assert_eq!(doc.image, "attachment://original.png");
    assert!(
        matches!(media::published(&doc, &message).unwrap().unwrap(), PublishedImage::Attachment { id, .. } if id.get()==50)
    );
}
#[test]
fn replacement_and_removal_discard_only_managed_main_image() {
    let message = original();
    for change in [
        ImageChange::Remove,
        ImageChange::External("https://example.org/new.png".into()),
    ] {
        let mut doc = original_doc();
        let edits =
            media::edit_attachments(&mut doc, &change, &message, Some(&metadata())).unwrap();
        assert_eq!(serde_json::to_value(edits).unwrap(), json!([{"id":"51"}]));
        assert_eq!(doc.image.is_empty(), matches!(change, ImageChange::Remove));
    }
    let upload = media::validate(
        &encoded(image::ImageFormat::Png, 8),
        "new.png",
        Some("image/png"),
        1024,
    )
    .unwrap();
    let mut doc = original_doc();
    let edits = serde_json::to_value(
        media::edit_attachments(
            &mut doc,
            &ImageChange::Upload(upload.clone()),
            &message,
            Some(&metadata()),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(edits[0]["id"], "51");
    assert_eq!(edits[1]["filename"], upload.attachment().filename);
    assert_eq!(doc.image, upload.image_url());
    let mut url_doc = Document {
        image: "https://example.org/old.png".into(),
        ..original_doc()
    };
    let edits = serde_json::to_value(
        media::edit_attachments(&mut url_doc, &ImageChange::Upload(upload), &message, None)
            .unwrap(),
    )
    .unwrap();
    assert_eq!(edits.as_array().unwrap().len(), 3);
}
#[test]
fn restart_keeps_metadata_and_preview_uses_fresh_public_message_attachment_url() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().join("data")).unwrap();
    let gid = s::GuildId::new(1);
    let mut state = State::default();
    state
        .documents
        .insert("announcement-1".into(), original_doc());
    state
        .announcement_images
        .insert("announcement-1".into(), metadata());
    store.write(gid, &state).unwrap();
    drop(store);
    let reopened = Store::open(dir.path().join("data")).unwrap();
    let state = reopened.read(gid).unwrap();
    let mut doc = state.documents["announcement-1"].clone();
    media::preview_image(&mut doc, &original(), &state, "announcement-1").unwrap();
    assert!(doc.image.ends_with("?ex=fresh"));
    assert_eq!(
        state.documents["announcement-1"].image,
        "attachment://original.png"
    );
    assert!(
        reopened
            .read(s::GuildId::new(2))
            .unwrap()
            .announcement_images
            .is_empty()
    );
    let bytes = std::fs::read_to_string(dir.path().join("data/guilds/1/state.json")).unwrap();
    assert!(!bytes.contains("base64"));
}
#[test]
fn missing_managed_file_blocks_keep_but_allows_removal() {
    let mut doc = original_doc();
    assert!(
        media::edit_attachments(
            &mut doc,
            &ImageChange::Keep,
            &s::Message::default(),
            Some(&metadata())
        )
        .is_err()
    );
    assert!(
        media::edit_attachments(
            &mut doc,
            &ImageChange::Remove,
            &s::Message::default(),
            Some(&metadata())
        )
        .is_ok()
    );
    assert!(doc.image.is_empty());
}
#[test]
fn legacy_signed_attachment_urls_are_recovered_from_original_message() {
    let mut doc = original_doc();
    doc.image = "https://cdn.discordapp.com/attachments/11/100/original.png?ex=expired".into();
    let kept = media::edit_attachments(&mut doc, &ImageChange::Keep, &original(), None).unwrap();
    assert_eq!(
        serde_json::to_value(kept)
            .unwrap()
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert_eq!(doc.image, "attachment://original.png");
}
#[test]
fn preview_embeds_upload_privately_with_publish_cancel_controls() {
    let bytes = encoded(image::ImageFormat::Png, 16);
    let upload = media::validate(&bytes, "banner.png", Some("image/png"), 1024).unwrap();
    let change = ImageChange::Upload(upload);
    let mut doc = original_doc();
    change.apply(&mut doc).unwrap();
    let reply = discord_server_bot::services::ui::confirmation_reply(
        123,
        "Prévia privada",
        "Publicar",
        Some((&doc, "announcements")),
        change.files(),
    )
    .unwrap();
    assert_eq!(reply.ephemeral, Some(true));
    assert_eq!(reply.attachments.len(), 1);
    assert_eq!(reply.attachments[0].data, bytes);
    let embed = serde_json::to_value(&reply.embeds[0]).unwrap();
    assert_eq!(embed["image"]["url"], doc.image);
    let controls = serde_json::to_value(reply.components.unwrap()).unwrap();
    assert_eq!(controls[0]["components"][0]["custom_id"], "confirm:123");
    assert_eq!(controls[0]["components"][1]["custom_id"], "cancel:123");
}
#[test]
fn slash_schema_exposes_optional_native_attachments_and_conditional_image_actions() {
    for root in ["announcement", "announce"] {
        let command = commands::all()
            .into_iter()
            .find(|c| c.name == root)
            .unwrap();
        let schema = serde_json::to_value(command.create_as_slash_command().unwrap()).unwrap();
        let options = schema["options"].as_array().unwrap();
        let image = options.iter().find(|o| o["name"] == "imagem").unwrap();
        assert_eq!(image["type"], 11);
        assert_eq!(image["required"], false);
    }
    assert!(
        permissions::conditional_parameters(
            "announcement",
            "edit",
            &["imagem", "url_imagem", "imagem_acao"]
        )
        .is_ok()
    );
    assert!(permissions::conditional_parameters("announcement", "remove", &["imagem"]).is_err());
    let mut doc = original_doc();
    ImageChange::Remove.apply(&mut doc).unwrap();
    assert_eq!(
        announcements::with_message(&doc, "Somente texto")
            .unwrap()
            .image,
        ""
    );
}
#[tokio::test]
async fn bounded_download_reports_http_and_size_errors() {
    for (status, body, limit, expected) in [
        (403, vec![1], 1024, "HTTP 403"),
        (200, vec![1; 100], 10, "acima do limite"),
    ] {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let server = std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            let mut buffer = [0; 4096];
            let _ = socket.read(&mut buffer).unwrap();
            write!(
                socket,
                "HTTP/1.1 {status} OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            )
            .unwrap();
            socket.write_all(&body).unwrap();
        });
        let response = reqwest::get(url).await.unwrap();
        assert!(
            discord_server_bot::services::attachments::read_response(response, limit)
                .await
                .unwrap_err()
                .to_string()
                .contains(expected)
        );
        server.join().unwrap();
    }
}
