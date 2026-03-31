use std::env;
use std::process;

use COMP_2432project::api::AppState;
use COMP_2432project::terminal_cli::run_cli;

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    if let Err(error) = run_cli(AppState::new(), &args) {
        eprintln!("Error: {error}");
        process::exit(1);
    }
}
