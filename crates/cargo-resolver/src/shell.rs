//! A stand-in for `cargo_util_terminal::Shell`, as far as the moved code
//! talks to one.
//!
//! The resolver prints a status line while a slow resolution runs. This type
//! has the methods Cargo's shell has for that, with Cargo's signatures, over a
//! writer the application chooses. `cargo-util-terminal` itself depends on
//! `cargo-util`, which does not build for WebAssembly. There is no color and no
//! progress bar.

use std::fmt;
use std::io::{IsTerminal, Write};

use annotate_snippets::{Group, Renderer};

use crate::util::errors::CargoResult;

/// The requested verbosity of output.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Verbosity {
    Verbose,
    Normal,
    Quiet,
}

/// Whether messages should use color output
#[derive(Debug, PartialEq, Clone, Copy)]
pub enum ColorChoice {
    /// Force color output
    Always,
    /// Force disable color output
    Never,
    /// Intelligently guess whether to use color output
    CargoAuto,
}

/// An abstraction around console output that remembers preferences for output
/// verbosity and color.
pub struct Shell {
    output: ShellOut,
    verbosity: Verbosity,
    color_choice: ColorChoice,
    needs_clear: bool,
}

enum ShellOut {
    /// A plain write object; `is_err_tty` is false.
    Write(Box<dyn Write + Send + Sync>),
    /// The process's stdout and stderr.
    Stream {
        stdout: std::io::Stdout,
        stderr: std::io::Stderr,
        stderr_tty: bool,
    },
}

impl Shell {
    /// Creates a new shell writing to the process's stdout and stderr, verbose.
    pub fn new() -> Shell {
        Shell {
            output: ShellOut::Stream {
                stdout: std::io::stdout(),
                stderr: std::io::stderr(),
                stderr_tty: std::io::stderr().is_terminal(),
            },
            verbosity: Verbosity::Verbose,
            color_choice: ColorChoice::CargoAuto,
            needs_clear: false,
        }
    }

    /// Creates a shell from a plain writable object, with no color, and max verbosity.
    pub fn from_write(out: Box<dyn Write + Send + Sync>) -> Shell {
        Shell {
            output: ShellOut::Write(out),
            verbosity: Verbosity::Verbose,
            color_choice: ColorChoice::Never,
            needs_clear: false,
        }
    }

    /// Sets whether the next print should clear the current line.
    pub fn set_needs_clear(&mut self, needs_clear: bool) {
        self.needs_clear = needs_clear;
    }

    /// Returns `true` if the `needs_clear` flag is unset.
    pub fn is_cleared(&self) -> bool {
        !self.needs_clear
    }

    /// Returns `true` if stderr is a tty.
    pub fn is_err_tty(&self) -> bool {
        match self.output {
            ShellOut::Stream { stderr_tty, .. } => stderr_tty,
            ShellOut::Write(_) => false,
        }
    }

    /// Gets a reference to the underlying stdout writer.
    pub fn out(&mut self) -> &mut dyn Write {
        match &mut self.output {
            ShellOut::Write(w) => w,
            ShellOut::Stream { stdout, .. } => stdout,
        }
    }

    /// Gets a reference to the underlying stderr writer.
    pub fn err(&mut self) -> &mut dyn Write {
        match &mut self.output {
            ShellOut::Write(w) => w,
            ShellOut::Stream { stderr, .. } => stderr,
        }
    }

    /// Erase from cursor to end of line. Nothing to erase here: there is no
    /// progress bar.
    pub fn err_erase_line(&mut self) {
        self.needs_clear = false;
    }

    /// Prints a right-aligned status line, as Cargo does without color.
    pub fn status<T, U>(&mut self, status: T, message: U) -> CargoResult<()>
    where
        T: fmt::Display,
        U: fmt::Display,
    {
        self.print(&status, Some(&message), true)
    }

    pub fn transient_status<T>(&mut self, status: T) -> CargoResult<()>
    where
        T: fmt::Display,
    {
        self.print(&status, None, true)
    }

    /// Runs the callback only if we are in verbose mode.
    pub fn verbose<F>(&mut self, mut callback: F) -> CargoResult<()>
    where
        F: FnMut(&mut Shell) -> CargoResult<()>,
    {
        match self.verbosity {
            Verbosity::Verbose => callback(self),
            _ => Ok(()),
        }
    }

    /// Runs the callback if we are not in verbose mode.
    pub fn concise<F>(&mut self, mut callback: F) -> CargoResult<()>
    where
        F: FnMut(&mut Shell) -> CargoResult<()>,
    {
        match self.verbosity {
            Verbosity::Verbose => Ok(()),
            _ => callback(self),
        }
    }

    /// Prints an `error:` message.
    pub fn error<T: fmt::Display>(&mut self, message: T) -> CargoResult<()> {
        self.print(&"error", Some(&message), false)
    }

    /// Prints a `warning:` message, unless quiet.
    pub fn warn<T: fmt::Display>(&mut self, message: T) -> CargoResult<()> {
        match self.verbosity {
            Verbosity::Quiet => Ok(()),
            _ => self.print(&"warning", Some(&message), false),
        }
    }

    /// Prints a `note:` message.
    pub fn note<T: fmt::Display>(&mut self, message: T) -> CargoResult<()> {
        self.print(&"note", Some(&message), false)
    }

    /// Updates the verbosity of the shell.
    pub fn set_verbosity(&mut self, verbosity: Verbosity) {
        self.verbosity = verbosity;
    }

    /// Gets the verbosity of the shell.
    pub fn verbosity(&self) -> Verbosity {
        self.verbosity
    }

    /// Updates the color choice (always, never, or auto) from a string.
    ///
    /// Remembered as Cargo does; nothing here prints color.
    pub fn set_color_choice(&mut self, color: Option<&str>) -> CargoResult<()> {
        self.color_choice = match color {
            Some("always") => ColorChoice::Always,
            Some("never") => ColorChoice::Never,
            Some("auto") | None => ColorChoice::CargoAuto,
            Some(arg) => anyhow::bail!(
                "argument for --color must be auto, always, or \
                 never, but found `{}`",
                arg
            ),
        };
        Ok(())
    }

    /// Gets the current color choice.
    pub fn color_choice(&self) -> ColorChoice {
        self.color_choice
    }

    /// Whether `stderr` supports color.
    pub fn err_supports_color(&self) -> bool {
        false
    }

    /// Prints the passed in report to stderr, as Cargo renders it without
    /// color.
    pub fn print_report(&mut self, report: &[Group<'_>], force: bool) -> CargoResult<()> {
        if !force && matches!(self.verbosity, Verbosity::Quiet) {
            return Ok(());
        }
        let rendered = Renderer::plain().render(report);
        self.err().write_all(rendered.as_bytes())?;
        self.err().write_all(b"\n")?;
        Ok(())
    }

    /// Prints a status and a message, right-aligned when `justified`.
    fn print(
        &mut self,
        status: &dyn fmt::Display,
        message: Option<&dyn fmt::Display>,
        justified: bool,
    ) -> CargoResult<()> {
        match self.verbosity {
            Verbosity::Quiet => Ok(()),
            _ => {
                let line = match (justified, message) {
                    (true, Some(message)) => format!("{status:>12} {message}\n"),
                    (true, None) => format!("{status:>12}\n"),
                    (false, Some(message)) => format!("{status}: {message}\n"),
                    (false, None) => format!("{status}:\n"),
                };
                self.err().write_all(line.as_bytes())?;
                Ok(())
            }
        }
    }
}

impl Default for Shell {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Debug for Shell {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Shell")
            .field("verbosity", &self.verbosity)
            .field("color_choice", &self.color_choice)
            .finish_non_exhaustive()
    }
}
