use super::*;

#[test]
fn preview_command_reports_unavailable_without_creating_output() -> Result<()> {
    let path = temp_db_path("preview-unavailable");
    let output_path = temp_artifact_path("preview-unavailable", ".jpg");
    let ids = seed_database(&path, &[text_snapshot(1, "no image derivative")])?;

    let output = run_cli(&[
        "--db",
        path.to_str().expect("db path should be UTF-8"),
        "preview",
        &ids[0].to_string(),
        "0",
        "public.utf8-plain-text",
        "--out",
        output_path.to_str().expect("output path should be UTF-8"),
        "--format",
        "json",
    ]);
    let json: serde_json::Value = serde_json::from_str(&stdout_text(&output))?;

    assert!(output.status.success());
    assert_eq!(json["status"], "unavailable");
    assert_eq!(json["available"], false);
    assert!(!output_path.exists());

    cleanup_db(&path);
    Ok(())
}
