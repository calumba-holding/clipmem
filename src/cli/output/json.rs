use super::*;

pub(in crate::cli) fn print_json<T: Serialize>(value: &T) -> Result<()> {
    let json = serde_json::to_string_pretty(value)?;
    println!("{json}");
    Ok(())
}

pub(in crate::cli) fn print_jsonl_list(envelope: &ListEnvelope) -> Result<()> {
    let meta = json!({
        "type": "meta",
        "schema_version": envelope.schema_version,
        "command": envelope.command,
        "generated_at": envelope.generated_at,
        "applied_filters": envelope.applied_filters,
        "truncated": envelope.truncated,
        "next_cursor": envelope.next_cursor,
    });
    println!("{}", serde_json::to_string(&meta)?);

    for row in &envelope.results {
        let mut line = serde_json::to_value(row)?;
        let object = line
            .as_object_mut()
            .ok_or_else(|| anyhow!("list row JSONL serialization did not produce an object"))?;
        object.insert("type".to_string(), Value::String("result".to_string()));
        println!("{}", serde_json::to_string(&line)?);
    }

    Ok(())
}
