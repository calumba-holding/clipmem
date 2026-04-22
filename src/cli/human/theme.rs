use super::*;

pub(in crate::cli) const WIDTH: usize = 96;
pub(in crate::cli) const BAR_WIDTH: usize = 18;

#[derive(Debug, Clone, Copy)]
pub(in crate::cli) struct HumanTheme {
    color: bool,
}

impl HumanTheme {
    pub(in crate::cli) fn detect() -> Self {
        let force_color = std::env::var_os("CLICOLOR_FORCE").is_some();
        let no_color = std::env::var_os("NO_COLOR").is_some();
        Self {
            color: force_color || (!no_color && std::io::stdout().is_terminal()),
        }
    }

    pub(in crate::cli) fn title(self, text: &str) -> String {
        if self.color {
            text.bold().green().to_string()
        } else {
            text.to_string()
        }
    }

    pub(in crate::cli) fn section(self, text: &str) -> String {
        if self.color {
            text.bold().to_string()
        } else {
            text.to_string()
        }
    }

    pub(in crate::cli) fn id(self, text: impl ToString) -> String {
        let text = text.to_string();
        if self.color {
            text.bright_cyan().bold().to_string()
        } else {
            text
        }
    }

    pub(in crate::cli) fn score(self, value: f64, text: &str) -> String {
        if !self.color {
            return text.to_string();
        }
        if value >= 0.8 {
            text.green().bold().to_string()
        } else if value >= 0.5 {
            text.yellow().bold().to_string()
        } else {
            text.red().bold().to_string()
        }
    }

    pub(in crate::cli) fn warning(self, text: &str) -> String {
        if self.color {
            text.yellow().bold().to_string()
        } else {
            text.to_string()
        }
    }

    pub(in crate::cli) fn bar(self, value: usize, max: usize, width: usize) -> String {
        let bar = bar(value, max, width);
        if self.color {
            bar.cyan().to_string()
        } else {
            bar
        }
    }
}
