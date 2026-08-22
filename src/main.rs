mod list;
mod puck;
mod theme;

use std::process::ExitCode;

use yankout::history::Backend;

enum Verb {
    /// The list window (no verb).
    Window,
    Puck,
    Watch,
    List,
    Decode(Option<String>),
}

fn main() -> ExitCode {
    let mut verb = Verb::Window;
    let mut css_path: Option<String> = None;
    let mut backend = Backend::Auto;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "watch" => verb = Verb::Watch,
            "list" => verb = Verb::List,
            "decode" => verb = Verb::Decode(args.next()),
            "--current" => verb = Verb::Puck,
            "--css" => match args.next() {
                Some(path) => css_path = Some(path),
                None => {
                    eprintln!("yankout: --css takes a file argument\n{USAGE}");
                    return ExitCode::from(2);
                }
            },
            "--backend" => match args.next().as_deref() {
                Some("cliphist") => backend = Backend::Cliphist,
                Some("native") => backend = Backend::Native,
                _ => {
                    eprintln!("yankout: --backend takes cliphist or native\n{USAGE}");
                    return ExitCode::from(2);
                }
            },
            "--help" | "-h" => {
                print!("{USAGE}");
                return ExitCode::SUCCESS;
            }
            other => {
                eprintln!("yankout: unknown argument {other}\n{USAGE}");
                return ExitCode::from(2);
            }
        }
    }

    // Flags only apply to the modes that use them; rejecting the rest
    // keeps a typo from silently doing nothing.
    let wants_css = matches!(verb, Verb::Window | Verb::Puck);
    let wants_backend = matches!(verb, Verb::Window | Verb::List | Verb::Decode(_));
    if (css_path.is_some() && !wants_css) || (backend != Backend::Auto && !wants_backend) {
        eprintln!("yankout: that flag does not apply to this mode\n{USAGE}");
        return ExitCode::from(2);
    }

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
            if matches!(verb, Verb::Puck) {
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
               watcher is running and cliphist otherwise
";
