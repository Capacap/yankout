mod list;
mod puck;
mod theme;

use std::process::ExitCode;

use yankout::history::Backend;

#[derive(Debug, PartialEq, Eq)]
enum Verb {
    /// The list window (no verb).
    Window,
    Puck,
    Watch,
    List,
    Decode(Option<String>),
}

#[derive(Debug, PartialEq, Eq)]
struct Invocation {
    verb: Verb,
    css_path: Option<String>,
    backend: Backend,
}

enum Parsed {
    Run(Invocation),
    Help,
}

/// Exactly one verb, and only the flags that verb uses: a flag that
/// silently did nothing would hide a typo.
fn parse(args: impl IntoIterator<Item = String>) -> Result<Parsed, String> {
    let mut args = args.into_iter().peekable();
    let mut verb: Option<Verb> = None;
    let mut css_path = None;
    let mut backend = Backend::Auto;
    let set_verb = |slot: &mut Option<Verb>, v: Verb| -> Result<(), String> {
        match slot {
            Some(_) => Err("only one mode per invocation".into()),
            None => {
                *slot = Some(v);
                Ok(())
            }
        }
    };
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "watch" => set_verb(&mut verb, Verb::Watch)?,
            "list" => set_verb(&mut verb, Verb::List)?,
            "decode" => {
                // a following flag is a flag, not the id
                let id = args.next_if(|a| !a.starts_with('-'));
                set_verb(&mut verb, Verb::Decode(id))?;
            }
            "--current" => set_verb(&mut verb, Verb::Puck)?,
            "--css" => match args.next() {
                Some(path) => css_path = Some(path),
                None => return Err("--css takes a file argument".into()),
            },
            "--backend" => match args.next().as_deref() {
                Some("cliphist") => backend = Backend::Cliphist,
                Some("native") => backend = Backend::Native,
                _ => return Err("--backend takes cliphist or native".into()),
            },
            "--help" | "-h" => return Ok(Parsed::Help),
            other => return Err(format!("unknown argument {other}")),
        }
    }
    let verb = verb.unwrap_or(Verb::Window);
    let wants_css = matches!(verb, Verb::Window | Verb::Puck);
    let wants_backend = matches!(verb, Verb::Window | Verb::List | Verb::Decode(_));
    if css_path.is_some() && !wants_css {
        return Err("--css does not apply to this mode".into());
    }
    if backend != Backend::Auto && !wants_backend {
        return Err("--backend does not apply to this mode".into());
    }
    Ok(Parsed::Run(Invocation {
        verb,
        css_path,
        backend,
    }))
}

fn main() -> ExitCode {
    let Invocation {
        verb,
        css_path,
        backend,
    } = match parse(std::env::args().skip(1)) {
        Ok(Parsed::Run(inv)) => inv,
        Ok(Parsed::Help) => {
            print!("{USAGE}");
            return ExitCode::SUCCESS;
        }
        Err(e) => {
            eprintln!("yankout: {e}\n{USAGE}");
            return ExitCode::from(2);
        }
    };

    let result = match verb {
        Verb::Watch => {
            let result = yankout::store::default_dir()
                .and_then(|dir| yankout::watch::run(dir, yankout::store::DEFAULT_CAP));
            // run() only returns on failure; its loop has no clean exit
            result.map(|()| ExitCode::FAILURE)
        }
        Verb::List => yankout::history::select(backend).and_then(|h| {
            let mut out = std::io::stdout().lock();
            yankout::terminal::list(h.as_ref(), &mut out).map(|()| ExitCode::SUCCESS)
        }),
        Verb::Decode(id) => yankout::history::select(backend).and_then(|h| {
            let mut out = std::io::stdout().lock();
            yankout::terminal::decode_stdin(h.as_ref(), id.as_deref(), &mut out)
                .map(|()| ExitCode::SUCCESS)
        }),
        Verb::Puck | Verb::Window => {
            let user_css = match theme::read_user_css(css_path.as_deref()) {
                Ok(css) => css,
                Err(e) => {
                    eprintln!("yankout: {e}");
                    return ExitCode::FAILURE;
                }
            };
            if verb == Verb::Puck {
                Ok(puck::run(user_css))
            } else {
                yankout::history::select(backend).map(|h| list::run(user_css, h))
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

const USAGE: &str = "\
usage: yankout [--css <file>] [--backend cliphist|native]
       yankout --current [--css <file>]
       yankout list [--backend cliphist|native]
       yankout decode [<id>] [--backend cliphist|native]
       yankout watch
  (no args)    list recent clipboard history: type to filter, Enter or
               double-click to recall, drag anywhere on the window to
               drag the selected entry out; closes on Esc, focus loss,
               or a completed drop
  --current    show a puck that drags the current clipboard out;
               exits after a successful drop, or on Esc
  list         print history as <id>TAB<preview> lines, newest first
  decode       write one entry's raw content to stdout; the id is the
               argument, or the first tab-field of the first stdin line,
               so a picked `list` line can be piped back whole
  watch        maintain yankout's own clipboard history via the
               data-control protocol, replacing cliphist's watchers
  --css <file> load this css on top of the default theme instead of
               $XDG_CONFIG_HOME/yankout/style.css
  --backend    history backend; the default picks native while a
               watcher is running, else cliphist if installed
";

#[cfg(test)]
mod tests {
    use super::*;

    fn run(args: &[&str]) -> Result<Invocation, String> {
        match parse(args.iter().map(|a| a.to_string()))? {
            Parsed::Run(inv) => Ok(inv),
            Parsed::Help => panic!("help"),
        }
    }

    #[test]
    fn no_args_is_the_window() {
        assert_eq!(run(&[]).unwrap().verb, Verb::Window);
    }

    #[test]
    fn decode_id_is_optional_and_never_a_flag() {
        assert_eq!(
            run(&["decode", "42"]).unwrap().verb,
            Verb::Decode(Some("42".into()))
        );
        let inv = run(&["decode", "--backend", "native"]).unwrap();
        assert_eq!(inv.verb, Verb::Decode(None));
        assert_eq!(inv.backend, Backend::Native);
        assert!(matches!(
            parse(["decode".to_string(), "--help".to_string()]),
            Ok(Parsed::Help)
        ));
    }

    #[test]
    fn two_verbs_are_rejected() {
        assert!(run(&["watch", "--current"]).is_err());
        assert!(run(&["list", "decode"]).is_err());
        assert!(run(&["--current", "list"]).is_err());
    }

    #[test]
    fn flags_are_scoped_to_their_modes() {
        assert!(run(&["watch", "--css", "x.css"]).is_err());
        assert!(run(&["list", "--css", "x.css"]).is_err());
        assert!(run(&["--current", "--backend", "native"]).is_err());
        assert!(run(&["--current", "--css", "x.css"]).is_ok());
        assert!(run(&["list", "--backend", "cliphist"]).is_ok());
    }

    #[test]
    fn flag_arguments_are_validated() {
        assert!(run(&["--css"]).is_err());
        assert!(run(&["--backend", "sqlite"]).is_err());
        assert!(run(&["--frobnicate"]).is_err());
    }
}
