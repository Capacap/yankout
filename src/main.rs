mod list;
mod puck;

use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        None => list::run(),
        Some("--current") => puck::run(),
        Some("--help" | "-h") => {
            print!("{USAGE}");
            ExitCode::SUCCESS
        }
        Some(other) => {
            eprintln!("clipdrag: unknown argument {other}\n{USAGE}");
            ExitCode::from(2)
        }
    }
}

const USAGE: &str = "\
usage: clipdrag [--current]
  (no args)   list recent clipboard history: type to filter, Enter or
              double-click to recall, drag anywhere on the window to
              drag the selected entry out; closes on Esc, focus loss,
              or a completed drop
  --current   show a puck that drags the current clipboard out;
              exits after a successful drop, or on Esc
";
