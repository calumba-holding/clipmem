use std::io::IsTerminal;

use super::fmt::bar;

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
            ansi(text, &[Style::Bold, Style::Green])
        } else {
            text.to_string()
        }
    }

    pub(in crate::cli) fn section(self, text: &str) -> String {
        if self.color {
            ansi(text, &[Style::Bold])
        } else {
            text.to_string()
        }
    }

    pub(in crate::cli) fn id(self, text: impl ToString) -> String {
        let text = text.to_string();
        if self.color {
            ansi(&text, &[Style::Bold, Style::BrightCyan])
        } else {
            text
        }
    }

    pub(in crate::cli) fn score(self, value: f64, text: &str) -> String {
        if !self.color {
            return text.to_string();
        }
        if value >= 0.8 {
            ansi(text, &[Style::Bold, Style::Green])
        } else if value >= 0.5 {
            ansi(text, &[Style::Bold, Style::Yellow])
        } else {
            ansi(text, &[Style::Bold, Style::Red])
        }
    }

    pub(in crate::cli) fn warning(self, text: &str) -> String {
        if self.color {
            ansi(text, &[Style::Bold, Style::Yellow])
        } else {
            text.to_string()
        }
    }

    pub(in crate::cli) fn bar(self, value: usize, max: usize, width: usize) -> String {
        let bar = bar(value, max, width);
        if self.color {
            ansi(&bar, &[Style::Cyan])
        } else {
            bar
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum Style {
    Bold,
    Green,
    Yellow,
    Red,
    Cyan,
    BrightCyan,
}

impl Style {
    const fn code(self) -> &'static str {
        match self {
            Self::Bold => "1",
            Self::Green => "32",
            Self::Yellow => "33",
            Self::Red => "31",
            Self::Cyan => "36",
            Self::BrightCyan => "96",
        }
    }
}

fn ansi(text: &str, styles: &[Style]) -> String {
    let codes = styles.iter().map(|style| style.code()).collect::<Vec<_>>();
    format!("\x1b[{}m{text}\x1b[0m", codes.join(";"))
}
