//! Throwaway: run the native watcher against a scratch store dir.
//! Usage: cargo run --example spike_watch -- <dir>

fn main() {
    let dir = std::env::args().nth(1).expect("usage: spike_watch <dir>");
    if let Err(e) = yankout::watch::run(dir.into(), 10) {
        eprintln!("spike_watch: {}", e.0);
        std::process::exit(1);
    }
}
