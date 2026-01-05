use std::io::{self, Write};
use termcolor::{Color, ColorChoice, ColorSpec, StandardStream, WriteColor};

pub(crate) fn stdout() -> StandardStream {
    StandardStream::stdout(ColorChoice::Auto)
}

pub(crate) fn writeln_colored(
    stream: &mut StandardStream,
    text: &str,
    color: Color,
) -> io::Result<()> {
    stream.set_color(ColorSpec::new().set_fg(Some(color)))?;
    writeln!(stream, "{text}")?;
    stream.reset()
}

pub(crate) fn write_bold(stream: &mut StandardStream, text: &str) -> io::Result<()> {
    stream.set_color(ColorSpec::new().set_bold(true))?;
    write!(stream, "{text}")?;
    stream.reset()
}

pub(crate) fn writeln_bold(stream: &mut StandardStream, text: &str) -> io::Result<()> {
    stream.set_color(ColorSpec::new().set_bold(true))?;
    writeln!(stream, "{text}")?;
    stream.reset()
}
