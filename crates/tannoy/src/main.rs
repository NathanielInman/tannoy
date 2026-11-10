//! `tannoy`: the Tannoy daemon, CLI and waybar module.

mod bar;
mod cli;
mod daemon;
mod dbus;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let code = match cli::run(&args) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("tannoy: {e:#}");
            1
        }
    };
    std::process::exit(code);
}
