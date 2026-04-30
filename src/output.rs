#![allow(dead_code)] // Helpers consumed by later UX-pass tasks.

use crate::config::ColorMode;
use std::io::{self, Write};
use termcolor::{Color, ColorChoice, ColorSpec, StandardStream, WriteColor};

/// Owns the per-invocation color preference. Constructed once at the top of a
/// subcommand handler and threaded through the helpers below.
pub(crate) struct Output {
    mode: ColorMode,
}

impl Output {
    pub(crate) fn new(mode: ColorMode) -> Self {
        Self { mode }
    }

    fn stderr(&self) -> StandardStream {
        StandardStream::stderr(self.mode.into())
    }

    fn stdout(&self) -> StandardStream {
        StandardStream::stdout(self.mode.into())
    }

    /// Bold heading line on stderr (e.g., "Sync Plan", "=== Plugins ===").
    /// Status decoration, not subcommand result data, so → stderr.
    pub(crate) fn heading(&self, text: &str) {
        let mut s = self.stderr();
        let _ = s.set_color(ColorSpec::new().set_bold(true));
        let _ = writeln!(s, "{text}");
        let _ = s.reset();
    }

    /// Indented status/info line on stderr. Default color.
    pub(crate) fn status(&self, text: &str) {
        let mut s = self.stderr();
        let _ = writeln!(s, "{text}");
    }

    /// Indented warning line on stderr, yellow when colors enabled.
    pub(crate) fn warn(&self, text: &str) {
        let mut s = self.stderr();
        let _ = s.set_color(ColorSpec::new().set_fg(Some(Color::Yellow)));
        let _ = writeln!(s, "{text}");
        let _ = s.reset();
    }

    /// Indented success line on stderr, green when colors enabled.
    pub(crate) fn success(&self, text: &str) {
        let mut s = self.stderr();
        let _ = s.set_color(ColorSpec::new().set_fg(Some(Color::Green)));
        let _ = writeln!(s, "{text}");
        let _ = s.reset();
    }

    /// Subcommand result data on stdout. Plain (no color decoration) — pipeable.
    pub(crate) fn out(&self, text: &str) {
        let mut s = io::stdout();
        let _ = writeln!(s, "{text}");
    }

    /// Colored result data on stdout (used for diff entries: + green, - red, ~ yellow).
    /// Most callers want `out` instead.
    pub(crate) fn out_colored(&self, text: &str, color: Color) {
        let mut s = self.stdout();
        let _ = s.set_color(ColorSpec::new().set_fg(Some(color)));
        let _ = writeln!(s, "{text}");
        let _ = s.reset();
    }

    /// Bold-emphasis text on stdout without a trailing newline. Used by diff for the
    /// "Comparing profiles: X vs Y" prefix where we want bold + plain on the same line.
    pub(crate) fn out_bold_inline(&self, text: &str) {
        let mut s = self.stdout();
        let _ = s.set_color(ColorSpec::new().set_bold(true));
        let _ = write!(s, "{text}");
        let _ = s.reset();
    }
}

impl From<ColorMode> for ColorChoice {
    fn from(mode: ColorMode) -> Self {
        match mode {
            ColorMode::Auto => ColorChoice::Auto,
            ColorMode::Always => ColorChoice::Always,
            ColorMode::Never => ColorChoice::Never,
        }
    }
}
