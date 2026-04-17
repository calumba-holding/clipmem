use anyhow::Result;

fn main() -> Result<()> {
    clipmem::run()
}

#[cfg(test)]
mod tests {
    #[test]
    fn binary_entrypoint_can_exercise_parse_failures_through_library_seam() {
        let error = clipmem::run_from(["clipmem", "get", "42", "--events", "0"])
            .expect_err("invalid binary arguments should surface as an error");

        assert!(error.to_string().contains("between 1 and 250"));
    }
}
