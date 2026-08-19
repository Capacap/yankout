mod list;
mod puck;
mod theme;

use std::process::ExitCode;

fn main() -> ExitCode {
    let mut current = false;
    let mut css_path: Option<String> = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--current" => current = true,
            "--css" => match args.next() {
                Some(path) => css_path = Some(path),
                None => {
                    eprintln!("yankout: --css takes a file argument\n{USAGE}");
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
        list::run(user_css)
    }
}

const USAGE: &str = "\
usage: yankout [--current] [--css <file>]
  (no args)    list recent clipboard history: type to filter, Enter or
               double-click to recall, drag anywhere on the window to
               drag the selected entry out; closes on Esc, focus loss,
               or a completed drop
  --current    show a puck that drags the current clipboard out;
               exits after a successful drop, or on Esc
  --css <file> load extra css on top of the default theme
";
