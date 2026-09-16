//! File uploads. Everything is stored under the data directory and served
//! back from `/uploads`, so instance content never depends on a third party.

use axum::extract::{Multipart, State};
use axum::Json;
use serde_json::{json, Value};
use std::io::Cursor;
use tokio::io::AsyncWriteExt;

use crate::access;
use crate::auth::Auth;
use crate::error::{AppError, AppResult};
use crate::ids;
use crate::perms;
use crate::state::AppState;

/// Types we are willing to store. SVG is excluded deliberately: it can carry
/// script, and it would be served from the instance's own origin.
const ALLOWED: &[(&str, &str)] = &[
    ("image/png", "png"),
    ("image/jpeg", "jpg"),
    ("image/gif", "gif"),
    ("image/webp", "webp"),
    ("video/mp4", "mp4"),
    ("video/webm", "webm"),
    ("audio/mpeg", "mp3"),
    ("audio/ogg", "ogg"),
    ("audio/wav", "wav"),
    ("application/pdf", "pdf"),
    ("text/plain", "txt"),
    ("application/zip", "zip"),
];

fn extension_for(content_type: &str) -> Option<&'static str> {
    ALLOWED
        .iter()
        .find(|(mime, _)| *mime == content_type)
        .map(|(_, ext)| *ext)
}

pub async fn upload(
    State(state): State<AppState>,
    auth: Auth,
    mut multipart: Multipart,
) -> AppResult<Json<Value>> {
    auth.require(perms::ATTACH_FILES)?;
    let instance = access::instance(&state).await?;
    let max_bytes = (instance.max_upload_mb.clamp(1, 500) * 1024 * 1024) as usize;

    let field = multipart
        .next_field()
        .await
        .map_err(|e| AppError::bad(format!("Upload failed: {e}")))?
        .ok_or_else(|| AppError::bad("No file was included in the upload."))?;

    let original_name = field.file_name().unwrap_or("file").to_string();
    let declared_type = field
        .content_type()
        .unwrap_or("application/octet-stream")
        .to_string();

    let data = field.bytes().await.map_err(|_| {
        AppError::TooLarge(format!(
            "That file is too large. The limit is {} MB.",
            instance.max_upload_mb
        ))
    })?;

    if data.len() > max_bytes {
        return Err(AppError::TooLarge(format!(
            "That file is too large. The limit is {} MB.",
            instance.max_upload_mb
        )));
    }
    if data.is_empty() {
        return Err(AppError::bad("That file is empty."));
    }

    let extension = extension_for(&declared_type).ok_or_else(|| {
        AppError::bad(format!(
            "{declared_type} files aren't allowed. Images, video, audio, PDFs, text and zips are."
        ))
    })?;

    // For images, decode enough to confirm the bytes really are what the
    // client claimed and to record dimensions for layout.
    let mut dimensions: Option<(i64, i64)> = None;
    if declared_type.starts_with("image/") {
        match image::ImageReader::new(Cursor::new(&data[..])).with_guessed_format() {
            Ok(reader) => match reader.into_dimensions() {
                Ok((w, h)) => dimensions = Some((w as i64, h as i64)),
                Err(_) => return Err(AppError::bad("That image file appears to be corrupt.")),
            },
            Err(_) => return Err(AppError::bad("That image file appears to be corrupt.")),
        }
    }

    let id = ids::new_id();
    let stored_name = format!("{id}.{extension}");
    let dir = state.config.uploads_dir();
    tokio::fs::create_dir_all(&dir)
        .await
        .map_err(|e| AppError::Internal(format!("could not create upload dir: {e}")))?;

    let path = format!("{dir}/{stored_name}");
    let mut file = tokio::fs::File::create(&path)
        .await
        .map_err(|e| AppError::Internal(format!("could not write upload: {e}")))?;
    file.write_all(&data)
        .await
        .map_err(|e| AppError::Internal(format!("could not write upload: {e}")))?;
    file.flush()
        .await
        .map_err(|e| AppError::Internal(format!("could not flush upload: {e}")))?;

    // Keep the display name but strip anything path-like out of it.
    let display_name: String = original_name
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or("file")
        .chars()
        .filter(|c| !c.is_control())
        .take(120)
        .collect();

    Ok(Json(json!({
        "id": id,
        "filename": if display_name.is_empty() { stored_name.clone() } else { display_name },
        "content_type": declared_type,
        "size": data.len() as i64,
        "width": dimensions.map(|d| d.0),
        "height": dimensions.map(|d| d.1),
        "url": format!("/uploads/{stored_name}"),
    })))
}
