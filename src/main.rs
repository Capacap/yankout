mod list;
mod puck;
mod theme;

use std::process::ExitCode;

use yankout::history::Backend;

fn main() -> ExitCode {
    let mut current = false;
    let mut watch = false;
    let mut css_path: Option<String> = None;
    let mut backend = Backend::Auto;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "watch" => watch = true,
            "--current" => current = true,
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

    if watch {
        if current || css_path.is_some() || backend != Backend::Auto {
            eprintln!("yankout: watch takes no other arguments\n{USAGE}");
            return ExitCode::from(2);
        }
        let result = yankout::store::default_dir()
            .and_then(|dir| yankout::watch::run(dir, yankout::store::DEFAULT_CAP));
        // run() only returns on failure; its loop has no clean exit
        if let Err(e) = result {
            eprintln!("yankout: {e}");
        }
        return ExitCode::FAILURE;
    }

    let user_css = match theme::read_user_css(css_path.as_deref()) {
        Ok(css) => css,
        Err(e) => {
            eprintln!("yankout: {e}");
            return ExitCode::FAILURE;
        }
    };

    if current {
        puck::run(user_css)
    } else {
        let backend = match yankout::history::select(backend) {
            Ok(b) => b,
            Err(e) => {
                eprintln!("yankout: {e}");
                return ExitCode::FAILURE;
            }
        };
        list::run(user_css, backend)
    }
}

const USAGE: &str = "\
usage: yankout [--current] [--css <file>] [--backend cliphist|native]
       yankout watch
  (no args)    list recent clipboard history: type to filter, Enter or
               double-click to recall, drag anywhere on the window to
               drag the selected entry out; closes on Esc, focus loss,
               or a completed drop
  --current    show a puck that drags the current clipboard out;
               exits after a successful drop, or on Esc
  --css <file> load extra css on top of the default theme
  --backend    history backend for list mode; the default picks native
               while a watcher is running and cliphist otherwise
  watch        maintain yankout's own clipboard history via the
               data-control protocol, replacing cliphist's watchers
";
