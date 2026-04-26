use super::*;
use std::io::{self, Write as IoWrite};

pub(in crate::cli) fn print_json<T: Serialize>(value: &T) -> Result<()> {
    let stdout = io::stdout();
    let mut handle = stdout.lock();
    write_json_pretty(&mut handle, value)
}

pub(in crate::cli) fn print_json_line<T: Serialize>(value: &T) -> Result<()> {
    let stdout = io::stdout();
    let mut handle = stdout.lock();
    write_json_line(&mut handle, value)
}

pub(in crate::cli) fn write_json_pretty<T: Serialize>(
    writer: &mut impl IoWrite,
    value: &T,
) -> Result<()> {
    serde_json::to_writer_pretty(&mut *writer, value)?;
    writer.write_all(b"\n")?;
    Ok(())
}

pub(in crate::cli) fn write_json_line<T: Serialize>(
    writer: &mut impl IoWrite,
    value: &T,
) -> Result<()> {
    serde_json::to_writer(&mut *writer, value)?;
    writer.write_all(b"\n")?;
    Ok(())
}

pub(in crate::cli) fn print_jsonl_list(envelope: &ListEnvelope) -> Result<()> {
    let stdout = io::stdout();
    let mut handle = stdout.lock();
    let meta = json!({
        "type": "meta",
        "schema_version": envelope.schema_version,
        "command": envelope.command,
        "generated_at": envelope.generated_at,
        "applied_filters": envelope.applied_filters,
        "truncated": envelope.truncated,
        "next_cursor": envelope.next_cursor,
    });
    write_json_line(&mut handle, &meta)?;

    for row in &envelope.results {
        let mut line = serde_json::to_value(row)?;
        let object = line
            .as_object_mut()
            .ok_or_else(|| anyhow!("list row JSONL serialization did not produce an object"))?;
        object.insert("type".to_string(), Value::String("result".to_string()));
        write_json_line(&mut handle, &line)?;
    }

    Ok(())
}
