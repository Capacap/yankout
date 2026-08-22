mod list;
mod provider;
mod puck;
mod theme;

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand, ValueEnum};

use yankout_core::history::Backend;

/// Clipboard history that leaves by drag.
///
/// With no verb, opens the history list: type to filter, Enter or
/// double-click to recall, drag anywhere on the window to drag the
/// selected entry out; closes on Esc, focus loss, or a completed drop.
#[derive(Parser, Debug, PartialEq, Eq)]
#[command(name = "yankout", version, args_conflicts_with_subcommands = true)]
struct Cli {
    #[command(subcommand)]
    verb: Option<Verb>,

    /// Show a puck that drags the current clipboard out; exits after a
    /// successful drop, or on Esc
    #[arg(long, conflicts_with = "backend")]
    current: bool,

    /// Load this CSS on top of the default theme instead of
    /// $XDG_CONFIG_HOME/yankout/style.css
    #[arg(long, value_name = "FILE")]
    css: Option<PathBuf>,

    /// History backend; the default picks native while a watcher is
    /// running, else cliphist if installed
    #[arg(long, value_enum)]
    backend: Option<BackendArg>,
}

#[derive(Subcommand, Debug, PartialEq, Eq)]
enum Verb {
    /// Print history as <id>TAB<preview> lines, newest first
    List {
        /// History backend (default: native while a watcher runs, else cliphist)
        #[arg(long, value_enum)]
        backend: Option<BackendArg>,
    },
    /// Write one entry's raw content to stdout
    ///
    /// The id is the argument, or the first tab-field of the first
    /// stdin line, so a picked `list` line can be piped back whole.
    Decode {
        /// Entry id as printed by `list`; read from stdin when omitted
        id: Option<String>,
        /// History backend (default: native while a watcher runs, else cliphist)
        #[arg(long, value_enum)]
        backend: Option<BackendArg>,
    },
    /// Maintain yankout's own clipboard history via the data-control
    /// protocol, replacing cliphist's watchers
    Watch,
}

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
enum BackendArg {
    Cliphist,
    Native,
}

fn backend(arg: Option<BackendArg>) -> Backend {
    match arg {
        None => Backend::Auto,
        Some(BackendArg::Cliphist) => Backend::Cliphist,
        Some(BackendArg::Native) => Backend::Native,
    }
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let result = match cli.verb {
        Some(Verb::Watch) => {
            let result = yankout_core::store::default_dir()
                .and_then(|dir| yankout_core::watch::run(dir, yankout_core::store::DEFAULT_CAP));
            // run() only returns on failure; its loop has no clean exit
            result.map(|()| ExitCode::FAILURE)
        }
        Some(Verb::List { backend: b }) => {
            yankout_core::history::select(backend(b)).and_then(|h| {
                let mut out = std::io::stdout().lock();
                yankout_core::terminal::list(h.as_ref(), &mut out).map(|()| ExitCode::SUCCESS)
            })
        }
        Some(Verb::Decode { id, backend: b }) => yankout_core::history::select(backend(b))
            .and_then(|h| {
                let mut out = std::io::stdout().lock();
                yankout_core::terminal::decode_stdin(h.as_ref(), id.as_deref(), &mut out)
                    .map(|()| ExitCode::SUCCESS)
            }),
        None => {
            let user_css = match theme::read_user_css(cli.css.as_deref()) {
                Ok(css) => css,
                Err(e) => {
                    eprintln!("yankout: {e}");
                    return ExitCode::FAILURE;
                }
            };
            if cli.current {
                Ok(puck::run(user_css))
            } else {
                yankout_core::history::select(backend(cli.backend)).map(|h| list::run(user_css, h))
            }
        }
    };
    match result {
        Ok(code) => code,
        Err(e) => {
            eprintln!("yankout: {e}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Result<Cli, clap::Error> {
        Cli::try_parse_from(std::iter::once("yankout").chain(args.iter().copied()))
    }

    #[test]
    fn no_args_is_the_window() {
        let cli = parse(&[]).unwrap();
        assert_eq!(cli.verb, None);
        assert!(!cli.current);
    }

    #[test]
    fn decode_id_is_optional_and_never_a_flag() {
        assert!(matches!(
            parse(&["decode", "42"]).unwrap().verb,
            Some(Verb::Decode { id: Some(id), .. }) if id == "42"
        ));
        assert!(matches!(
            parse(&["decode", "--backend", "native"]).unwrap().verb,
            Some(Verb::Decode {
                id: None,
                backend: Some(BackendArg::Native)
            })
        ));
        assert_eq!(
            parse(&["decode", "--help"]).unwrap_err().kind(),
            clap::error::ErrorKind::DisplayHelp
        );
    }

    #[test]
    fn two_verbs_are_rejected() {
        assert!(parse(&["watch", "--current"]).is_err());
        assert!(parse(&["list", "decode"]).is_err());
        assert!(parse(&["--current", "list"]).is_err());
    }

    #[test]
    fn flags_are_scoped_to_their_modes() {
        assert!(parse(&["watch", "--css", "x.css"]).is_err());
        assert!(parse(&["list", "--css", "x.css"]).is_err());
        assert!(parse(&["--css", "x.css", "list"]).is_err());
        assert!(parse(&["--current", "--backend", "native"]).is_err());
        assert!(parse(&["--current", "--css", "x.css"]).is_ok());
        assert!(parse(&["list", "--backend", "cliphist"]).is_ok());
    }

    #[test]
    fn flag_arguments_are_validated() {
        assert!(parse(&["--css"]).is_err());
        assert!(parse(&["--backend", "sqlite"]).is_err());
        assert!(parse(&["--frobnicate"]).is_err());
    }
}
