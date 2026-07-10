from __future__ import annotations
import argparse
import json
import os
import sqlite3
from pathlib import Path


def resolve_source_root() -> Path:
    parser = argparse.ArgumentParser(description='Run Clipmem schema/invariant probes against the real schema.sql')
    parser.add_argument('--source-root', type=Path, help='Path to the extracted Clipmem repository root')
    args = parser.parse_args()
    candidates = [
        args.source_root,
        Path(os.environ['CLIPMEM_SOURCE_ROOT']) if os.environ.get('CLIPMEM_SOURCE_ROOT') else None,
        Path.cwd(),
        Path('/mnt/data/clipmem-audit/source/clipmem-main'),
    ]
    for candidate in candidates:
        if candidate is not None and (candidate / 'src/db/schema.sql').is_file():
            return candidate.resolve()
    parser.error('could not find src/db/schema.sql; pass --source-root /path/to/clipmem')


ROOT = resolve_source_root()
SCHEMA = (ROOT / 'src/db/schema.sql').read_text()
SEP = chr(31)

def scalar(conn, sql, params=()):
    return conn.execute(sql, params).fetchone()[0]

results = {}
conn = sqlite3.connect(':memory:')
conn.execute('PRAGMA foreign_keys=ON')
conn.executescript(SCHEMA)
results['sqlite_version'] = sqlite3.sqlite_version
results['schema_reapply'] = 'ok'
conn.executescript(SCHEMA)

# Seed an image-only snapshot with a placeholder preview, no searchable text.
conn.execute("INSERT INTO snapshots(id, sha256, snapshot_kind, preview_text, search_text, item_count, total_bytes) VALUES(1,'sha-image','image','[image · 12 bytes]','',1,12)")
conn.execute("INSERT INTO snapshot_items(snapshot_id,item_index,primary_kind,primary_uti,preview_text,search_text,total_bytes) VALUES(1,0,'image','public.png','[image · 12 bytes]','',12)")
conn.execute("INSERT INTO item_representations(snapshot_id,item_index,uti,kind,byte_len,raw_sha256,text_value,blob_value) VALUES(1,0,'public.png','image',12,'raw-image',NULL,zeroblob(12))")
conn.execute("INSERT INTO capture_events(snapshot_id,observed_at,change_count,frontmost_app_bundle_id,frontmost_app_name) VALUES(1,'2026-01-01T00:00:00Z',1,'com.example.one','First App')")

has_text_expr = """
(
 (s.preview_text IS NOT NULL AND s.preview_text != '')
 OR EXISTS (
   SELECT 1 FROM item_representations ir
   WHERE ir.snapshot_id=s.id
     AND ir.kind IN ('plain_text','url','file_url','html','json','xml','rtf')
     AND ir.text_value IS NOT NULL AND ir.text_value != ''
 )
 OR EXISTS (
   SELECT 1 FROM snapshot_ocr_cache soc
   WHERE soc.snapshot_id=s.id AND soc.ocr_text != ''
 )
)
"""
results['image_only_matches_has_text'] = bool(scalar(conn, f'SELECT {has_text_expr} FROM snapshots s WHERE s.id=1'))

# A representation can reference an item_index that does not exist because its only FK is snapshot_id.
orphan_insert = 'allowed'
try:
    conn.execute("INSERT INTO item_representations(snapshot_id,item_index,uti,kind,byte_len,raw_sha256,text_value,blob_value) VALUES(1,99,'public.data','binary',1,'orphan',NULL,x'00')")
except sqlite3.IntegrityError as exc:
    orphan_insert = f'rejected: {exc}'
results['orphan_representation_insert'] = orphan_insert
results['foreign_key_check_after_orphan'] = conn.execute('PRAGMA foreign_key_check').fetchall()
results['orphan_join_visible_to_item_loader_shape'] = scalar(conn, """
SELECT COUNT(*) FROM snapshot_items si
JOIN item_representations ir
  ON ir.snapshot_id=si.snapshot_id AND ir.item_index=si.item_index
WHERE si.snapshot_id=1 AND ir.item_index=99
""")

# Add a later capture from a different app. Event cache should retain both; literal haystack only last app.
conn.execute("INSERT INTO capture_events(snapshot_id,observed_at,change_count,frontmost_app_bundle_id,frontmost_app_name) VALUES(1,'2026-01-02T00:00:00Z',2,'com.example.two','Second App')")
row = conn.execute("SELECT app_names_lower,bundle_ids_lower FROM snapshot_event_filter_cache WHERE snapshot_id=1").fetchone()
haystack = scalar(conn, 'SELECT haystack FROM snapshot_literal_cache WHERE snapshot_id=1')
results['event_filter_cache_apps'] = row[0].split(SEP)
results['event_filter_cache_bundles'] = row[1].split(SEP)
results['literal_haystack'] = haystack
results['literal_haystack_contains_first_app'] = 'first app' in haystack
results['literal_haystack_contains_second_app'] = 'second app' in haystack
results['app_filter_matches_first_app'] = bool(scalar(conn, "SELECT app_names_lower LIKE '%first app%' FROM snapshot_event_filter_cache WHERE snapshot_id=1"))

# A pending restore is one-shot: first matching capture is ignored and marker deleted; second stores.
conn.execute("INSERT INTO pending_restores(snapshot_sha256,created_at) VALUES('sha-image',CURRENT_TIMESTAMP)")
before_events = scalar(conn, 'SELECT COUNT(*) FROM capture_events WHERE snapshot_id=1')
conn.execute("INSERT INTO capture_events(snapshot_id,change_count,frontmost_app_bundle_id,frontmost_app_name) VALUES(1,3,'com.example.clipmem','Clipmem')")
after_first = scalar(conn, 'SELECT COUNT(*) FROM capture_events WHERE snapshot_id=1')
marker_after_first = scalar(conn, "SELECT COUNT(*) FROM pending_restores WHERE snapshot_sha256='sha-image'")
conn.execute("INSERT INTO capture_events(snapshot_id,change_count,frontmost_app_bundle_id,frontmost_app_name) VALUES(1,4,'com.example.clipmem','Clipmem')")
after_second = scalar(conn, 'SELECT COUNT(*) FROM capture_events WHERE snapshot_id=1')
results['restore_suppression'] = {
    'events_before': before_events,
    'events_after_first_insert': after_first,
    'marker_after_first_insert': marker_after_first,
    'events_after_second_insert': after_second,
}

# The cached projections and indexes are internally populated for the valid representation.
results['cache_counts'] = {
    'snapshot_stats': scalar(conn, 'SELECT COUNT(*) FROM snapshot_stats'),
    'projection_cache': scalar(conn, 'SELECT COUNT(*) FROM snapshot_projection_cache'),
    'event_filter_cache': scalar(conn, 'SELECT COUNT(*) FROM snapshot_event_filter_cache'),
    'literal_cache': scalar(conn, 'SELECT COUNT(*) FROM snapshot_literal_cache'),
    'snapshots_fts': scalar(conn, 'SELECT COUNT(*) FROM snapshots_fts'),
    'snapshots_literal_fts': scalar(conn, 'SELECT COUNT(*) FROM snapshots_literal_fts'),
}

# Foreign-key definition inspection proves no composite FK to snapshot_items.
results['item_representation_foreign_keys'] = [dict(zip(['id','seq','table','from','to','on_update','on_delete','match'], row)) for row in conn.execute("PRAGMA foreign_key_list('item_representations')")]

print(json.dumps(results, indent=2, sort_keys=True))
