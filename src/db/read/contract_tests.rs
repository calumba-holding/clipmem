use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use anyhow::Result;
use rusqlite::hooks::{AuthAction, AuthContext, Authorization};

use crate::db::Database;
use crate::model::{build_item, build_representation, build_snapshot, CaptureContext};

#[test]
fn metadata_documents_and_revision_never_authorize_blob_reads() -> Result<()> {
    let mut db = Database::open_in_memory()?;
    let snapshot = build_snapshot(
        CaptureContext::new(1),
        vec![build_item(
            0,
            vec![build_representation(
                "public.data".to_string(),
                None,
                b"seed".to_vec(),
            )],
        )],
    );
    let stored = db.store_capture(&snapshot)?;
    db.conn.execute(
        "UPDATE item_representations SET blob_value = zeroblob(100000000), byte_len = 100000000 WHERE snapshot_id = ?1",
        [stored.snapshot_id()],
    )?;
    db.conn
        .authorizer(Some(|context: AuthContext<'_>| match context.action {
            AuthAction::Read {
                table_name: "item_representations",
                column_name: "blob_value",
            } => Authorization::Deny,
            _ => Authorization::Allow,
        }))?;

    let metadata = db
        .find_snapshot_metadata(stored.snapshot_id(), 10)?
        .expect("snapshot metadata exists");
    assert_eq!(
        metadata.items()[0].representations()[0].byte_len(),
        100_000_000
    );
    assert_eq!(
        db.find_snapshot_documents(&[stored.snapshot_id(), stored.snapshot_id()])?
            .len(),
        1
    );
    let _ = db.archive_revision()?;
    assert!(db
        .read_representation_payload(stored.snapshot_id(), 0, "public.data")
        .is_err());
    Ok(())
}

#[test]
fn snapshot_metadata_serialization_preserves_legacy_get_shape() -> Result<()> {
    let mut db = Database::open_in_memory()?;
    let stored = db.store_capture(&build_snapshot(
        CaptureContext::new(7),
        vec![build_item(
            0,
            vec![build_representation(
                "public.utf8-plain-text".to_string(),
                None,
                b"json contract".to_vec(),
            )],
        )],
    ))?;
    let legacy = db
        .find_snapshot(stored.snapshot_id(), 10)?
        .expect("legacy details exist");
    let metadata = db
        .find_snapshot_metadata(stored.snapshot_id(), 10)?
        .expect("metadata exists");
    assert_eq!(
        serde_json::to_value(legacy)?,
        serde_json::to_value(metadata)?
    );
    Ok(())
}

#[test]
fn bulk_documents_deduplicate_ids_and_chunk_above_sqlite_bind_limits() -> Result<()> {
    let mut db = Database::open_in_memory()?;
    let stored = db.store_capture(&build_snapshot(
        CaptureContext::new(1),
        vec![build_item(
            0,
            vec![build_representation(
                "public.utf8-plain-text".to_string(),
                None,
                b"bulk document".to_vec(),
            )],
        )],
    ))?;
    let mut ids = (10_000..11_100).collect::<Vec<_>>();
    ids.extend(std::iter::repeat_n(stored.snapshot_id(), 500));
    let documents = db.find_snapshot_documents(&ids)?;
    assert_eq!(documents.len(), 1);
    assert_eq!(
        documents[&stored.snapshot_id()].best_text(),
        "bulk document"
    );
    Ok(())
}

#[test]
fn forty_snapshot_document_load_is_two_bounded_blob_free_queries() -> Result<()> {
    let mut db = Database::open_in_memory()?;
    let mut ids = Vec::new();
    for change_count in 1..=40 {
        ids.push(
            db.store_capture(&build_snapshot(
                CaptureContext::new(change_count),
                vec![build_item(
                    0,
                    vec![build_representation(
                        "public.utf8-plain-text".to_string(),
                        None,
                        format!("document {change_count}").into_bytes(),
                    )],
                )],
            ))?
            .snapshot_id(),
        );
    }
    let selects = Arc::new(AtomicUsize::new(0));
    let observed = Arc::clone(&selects);
    db.conn
        .authorizer(Some(move |context: AuthContext<'_>| match context.action {
            AuthAction::Read {
                table_name: "item_representations",
                column_name: "blob_value",
            } => Authorization::Deny,
            AuthAction::Select => {
                observed.fetch_add(1, Ordering::SeqCst);
                Authorization::Allow
            }
            _ => Authorization::Allow,
        }))?;

    assert_eq!(db.find_snapshot_documents(&ids)?.len(), 40);
    assert_eq!(selects.load(Ordering::SeqCst), 2);
    Ok(())
}

#[test]
fn restore_payload_authorizes_blob_column_once() -> Result<()> {
    let mut db = Database::open_in_memory()?;
    let stored = db.store_capture(&build_snapshot(
        CaptureContext::new(1),
        vec![build_item(
            0,
            vec![build_representation(
                "public.data".to_string(),
                None,
                vec![1, 2, 3, 4],
            )],
        )],
    ))?;
    let blob_reads = Arc::new(AtomicUsize::new(0));
    let observed = Arc::clone(&blob_reads);
    db.conn.authorizer(Some(move |context: AuthContext<'_>| {
        if matches!(
            context.action,
            AuthAction::Read {
                table_name: "item_representations",
                column_name: "blob_value"
            }
        ) {
            observed.fetch_add(1, Ordering::SeqCst);
        }
        Authorization::Allow
    }))?;
    let payload = db
        .load_restore_payload(stored.snapshot_id())?
        .expect("restore payload exists");
    assert_eq!(
        payload.items()[0].representations()[0].raw_bytes(),
        &[1, 2, 3, 4]
    );
    assert_eq!(blob_reads.load(Ordering::SeqCst), 1);
    Ok(())
}
