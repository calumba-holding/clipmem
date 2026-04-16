use anyhow::Result;

fn main() -> Result<()> {
    clipmem::run()
}

#[cfg(test)]
mod tests {
    use anyhow::Result;

    #[test]
    fn main_entrypoint_is_exposed() {
        let function: fn() -> Result<()> = super::main;

        let _ = function;
    }
}
