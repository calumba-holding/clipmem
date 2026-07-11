use super::*;

#[test]
fn doctor_only_runs_invariant_scans_when_requested() {
    let path = temp_db_path("doctor-text");
    drop(Database::open_or_init(&path).expect("test database should open"));
    let output = run_cli(&[
        "--db",
        path.to_str().expect("db path should be UTF-8"),
        "doctor",
    ]);
    let stdout = stdout_text(&output);
    assert!(output.status.success());
    assert!(stdout.contains("database:"));
    assert!(stdout.contains("sqlite version:"));
    assert!(stdout.contains("fts5 temp table creation works:"));
    assert!(stdout.contains("integrity checks: skipped"));
    assert!(!stdout.contains("projection mismatches:"));

    let verified = run_cli(&[
        "--db",
        path.to_str().expect("db path should be UTF-8"),
        "doctor",
        "--verify-invariants",
    ]);
    let verified_stdout = stdout_text(&verified);
    assert!(verified.status.success());
    assert!(verified_stdout.contains("orphan representations: 0"));
    assert!(verified_stdout.contains("foreign key violations: 0"));
    assert!(verified_stdout.contains("projection mismatches: 0"));
    cleanup_db(&path);
}
