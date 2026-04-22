pub(in crate::db::tests) use std::sync::{Arc, Barrier};
pub(in crate::db::tests) use std::thread;
pub(in crate::db::tests) use std::time::Instant;

pub(in crate::db::tests) use anyhow::{Context, Result};
pub(in crate::db::tests) use std::io::Cursor;
pub(in crate::db::tests) use std::process;
pub(in crate::db::tests) use std::time::{SystemTime, UNIX_EPOCH};

pub(in crate::db::tests) use crate::model::{
    build_item, build_representation, build_snapshot, CaptureContext, ClipboardSnapshot,
};
pub(in crate::db::tests) use image::{ImageFormat, ImageReader, Rgba, RgbaImage};
pub(in crate::db::tests) use rusqlite::params;

pub(in crate::db::tests) use super::super::{
    collect_rows, configure_connection, explain_query_plan, Database, RetrievalFilters,
    RetrievalKind, SearchMode, TimelineSort, CURRENT_SCHEMA_VERSION, SCHEMA,
};

pub(in crate::db::tests) type RepresentationCompressionRow = (
    String,
    String,
    Option<String>,
    Option<i64>,
    Option<String>,
    i64,
    String,
    Vec<u8>,
);

pub(in crate::db::tests) fn temp_db_path(test_name: &str) -> std::path::PathBuf {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock should be after unix epoch")
        .as_nanos();

    std::env::temp_dir()
        .join("clipmem-db-tests")
        .join(format!("{test_name}-{}-{timestamp}.sqlite3", process::id()))
}

pub(in crate::db::tests) fn cleanup_db(path: &std::path::Path) {
    for suffix in ["", "-shm", "-wal"] {
        let candidate = if suffix.is_empty() {
            path.to_path_buf()
        } else {
            std::path::PathBuf::from(format!("{}{suffix}", path.display()))
        };
        let _ = std::fs::remove_file(candidate);
    }

    if let Some(parent) = path.parent() {
        let _ = std::fs::remove_dir(parent);
    }
}

pub(in crate::db::tests) fn fake_snapshot(change_count: i64, text: &str) -> ClipboardSnapshot {
    fake_snapshot_with_app(change_count, text, "Test App", "com.example.test")
}

pub(in crate::db::tests) fn fake_snapshot_with_app(
    change_count: i64,
    text: &str,
    app_name: &str,
    bundle_id: &str,
) -> ClipboardSnapshot {
    let representation = build_representation(
        "public.utf8-plain-text".to_string(),
        None,
        text.as_bytes().to_vec(),
    );

    let item = build_item(0, vec![representation]);
    build_snapshot(
        CaptureContext::new(change_count)
            .with_frontmost_app_name(app_name)
            .with_frontmost_app_bundle_id(bundle_id),
        vec![item],
    )
}

pub(in crate::db::tests) fn image_snapshot(
    change_count: i64,
    representations: Vec<(&str, Vec<u8>)>,
) -> ClipboardSnapshot {
    let items = representations
        .into_iter()
        .enumerate()
        .map(|(index, (uti, bytes))| {
            build_item(
                index,
                vec![build_representation(uti.to_string(), None, bytes)],
            )
        })
        .collect();
    build_snapshot(
        CaptureContext::new(change_count)
            .with_frontmost_app_name("Preview")
            .with_frontmost_app_bundle_id("com.apple.Preview"),
        items,
    )
}

pub(in crate::db::tests) fn lossless_test_tiff() -> Result<Vec<u8>> {
    let mut image = RgbaImage::new(256, 256);
    for (x, y, pixel) in image.enumerate_pixels_mut() {
        let alpha = if (x + y) % 3 == 0 { 96 } else { 255 };
        *pixel = Rgba([(x % 16) as u8, (y % 16) as u8, ((x + y) % 16) as u8, alpha]);
    }

    let mut out = Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(image).write_to(&mut out, ImageFormat::Tiff)?;
    Ok(out.into_inner())
}

pub(in crate::db::tests) fn decode_rgba(bytes: &[u8]) -> Result<Vec<u8>> {
    Ok(ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()?
        .decode()?
        .to_rgba8()
        .into_raw())
}

pub(in crate::db::tests) fn set_event_observed_at(
    db: &Database,
    event_id: i64,
    observed_at: &str,
) -> Result<()> {
    db.conn.execute(
        "UPDATE capture_events SET observed_at = ?1 WHERE id = ?2",
        [observed_at, &event_id.to_string()],
    )?;
    Ok(())
}

pub(in crate::db::tests) fn set_event_app(
    db: &Database,
    event_id: i64,
    app_name: Option<&str>,
    bundle_id: Option<&str>,
) -> Result<()> {
    db.conn.execute(
        "UPDATE capture_events
         SET frontmost_app_name = ?1, frontmost_app_bundle_id = ?2
         WHERE id = ?3",
        params![app_name, bundle_id, event_id],
    )?;
    Ok(())
}

pub(in crate::db::tests) fn unfiltered() -> RetrievalFilters {
    RetrievalFilters::default()
}

pub(in crate::db::tests) fn filters_with_app(app: &str) -> RetrievalFilters {
    RetrievalFilters::new(
        None,
        None,
        None,
        Some(app.to_string()),
        None,
        None,
        false,
        false,
        false,
        false,
        false,
        None,
        None,
    )
}

pub(in crate::db::tests) fn filters_with_kind(kind: RetrievalKind) -> RetrievalFilters {
    RetrievalFilters::new(
        None,
        None,
        None,
        None,
        None,
        Some(kind),
        false,
        false,
        false,
        false,
        false,
        None,
        None,
    )
}

pub(in crate::db::tests) fn seed_large_archive(
    db: &mut Database,
    snapshot_count: usize,
    event_count: usize,
) -> Result<()> {
    let tx = db
        .conn
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;

    {
        let mut insert_snapshot = tx.prepare(
            "INSERT INTO snapshots (
                sha256,
                snapshot_kind,
                preview_text,
                search_text,
                item_count,
                total_bytes,
                created_at
            ) VALUES (?1, 'plain_text', ?2, ?3, 1, 96, ?4)",
        )?;
        let mut insert_item = tx.prepare(
            "INSERT INTO snapshot_items (
                snapshot_id,
                item_index,
                primary_kind,
                primary_uti,
                preview_text,
                search_text,
                total_bytes
            ) VALUES (?1, 0, 'plain_text', 'public.utf8-plain-text', ?2, ?3, 96)",
        )?;
        let mut insert_text_representation = tx.prepare(
            "INSERT INTO item_representations (
                snapshot_id,
                item_index,
                uti,
                kind,
                byte_len,
                raw_sha256,
                text_value,
                blob_value
            ) VALUES (?1, 0, 'public.utf8-plain-text', 'plain_text', 96, ?2, ?3, ?4)",
        )?;
        let mut insert_url_representation = tx.prepare(
            "INSERT INTO item_representations (
                snapshot_id,
                item_index,
                uti,
                kind,
                byte_len,
                raw_sha256,
                text_value,
                blob_value
            ) VALUES (?1, 0, 'public.url', 'url', 48, ?2, ?3, ?4)",
        )?;

        for snapshot_index in 0..snapshot_count {
            let snapshot_number = snapshot_index + 1;
            let preview_text = format!("git status output sample {snapshot_number}");
            let search_text =
                format!("{preview_text} with repo path /tmp/repo/{snapshot_number}/Cargo.toml");
            let url = format!("https://example.com/{snapshot_number}");
            let created_at = format!(
                "2026-04-17 {:02}:{:02}:{:02}",
                12usize.saturating_sub((snapshot_index / 3600) % 12),
                (snapshot_index / 60) % 60,
                snapshot_index % 60
            );
            let fingerprint = format!("{snapshot_number:064x}");

            insert_snapshot
                .execute(params![fingerprint, preview_text, search_text, created_at,])?;

            let snapshot_id = tx.last_insert_rowid();
            insert_item.execute(params![snapshot_id, preview_text, search_text])?;

            let text_bytes = search_text.as_bytes().to_vec();
            insert_text_representation.execute(params![
                snapshot_id,
                format!("{:064x}", 100_000 + snapshot_number),
                search_text,
                text_bytes,
            ])?;

            let url_bytes = url.as_bytes().to_vec();
            insert_url_representation.execute(params![
                snapshot_id,
                format!("{:064x}", 200_000 + snapshot_number),
                url,
                url_bytes,
            ])?;
        }
    }

    {
        let mut insert_event = tx.prepare(
            "INSERT INTO capture_events (
                snapshot_id,
                observed_at,
                change_count,
                frontmost_app_bundle_id,
                frontmost_app_name
            ) VALUES (?1, ?2, ?3, ?4, ?5)",
        )?;

        for event_index in 0..event_count {
            let snapshot_id = ((event_index % snapshot_count) + 1) as i64;
            let observed_at = format!(
                "2026-04-17 {:02}:{:02}:{:02}",
                12usize.saturating_sub((event_index / 3600) % 12),
                (event_index / 60) % 60,
                event_index % 60
            );
            let (bundle_id, app_name) = if event_index % 3 == 0 {
                ("com.apple.Terminal", "Terminal")
            } else {
                ("com.apple.Safari", "Safari")
            };

            insert_event.execute(params![
                snapshot_id,
                observed_at,
                event_index as i64 + 1,
                bundle_id,
                app_name,
            ])?;
        }
    }

    tx.execute_batch("ANALYZE")?;
    tx.commit()?;
    Ok(())
}
