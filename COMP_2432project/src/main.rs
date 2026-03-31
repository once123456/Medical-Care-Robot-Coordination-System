//! HTTP API entry point for the scheduler demo.
//! Serves as the user-space entry, launching an axum-based HTTP server
//! that exposes the kernel coordinator's state as JSON for the frontend dashboard.

use std::env;
use std::net::SocketAddr;
use std::process::ExitCode;

use axum::Router;
use tower_http::cors::{Any, CorsLayer};

use COMP_2432project::api::{AppState, build_router};
use COMP_2432project::terminal_cli::{print_usage, run_interactive_loop};

#[tokio::main]
async fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        print_usage();
        println!();
        println!("Server options:");
        println!("  --server-only   Start the HTTP API without the interactive terminal CLI");
        return ExitCode::SUCCESS;
    }

    let server_only = args.iter().any(|arg| arg == "--server-only");
    let state = AppState::new();

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let app: Router = build_router(state.clone()).layer(cors);

    let port = env::var("PORT")
        .ok()
        .and_then(|value| value.parse::<u16>().ok())
        .unwrap_or(3000);
    let addr: SocketAddr = format!("0.0.0.0:{port}")
        .parse()
        .expect("valid socket address");

    let listener = match tokio::net::TcpListener::bind(addr).await {
        Ok(listener) => listener,
        Err(error) if error.kind() == std::io::ErrorKind::AddrInUse => {
            eprintln!(
                "Failed to start HTTP API: port {port} is already in use. Stop the existing server or run with PORT=<other-port>."
            );
            return ExitCode::FAILURE;
        }
        Err(error) => {
            eprintln!("Failed to start HTTP API on http://{addr}: {error}");
            return ExitCode::FAILURE;
        }
    };

    println!("HTTP API server listening on http://{addr}");
    if server_only {
        println!("Interactive terminal CLI disabled (--server-only).");
    } else {
        println!("Interactive terminal CLI is active in this terminal. The frontend can stay connected at the same time.");
    }

    if server_only {
        if let Err(error) = axum::serve(listener, app).await {
            eprintln!("HTTP API server exited with error: {error}");
            return ExitCode::FAILURE;
        }

        return ExitCode::SUCCESS;
    }

    let server_handle = tokio::spawn(async move { axum::serve(listener, app).await });

    if let Err(error) = run_interactive_loop(state) {
        eprintln!("Interactive CLI stopped: {error}");
    }

    match server_handle.await {
        Ok(Ok(())) => ExitCode::SUCCESS,
        Ok(Err(error)) => {
            eprintln!("HTTP API server exited with error: {error}");
            ExitCode::FAILURE
        }
        Err(error) => {
            eprintln!("HTTP API server task failed: {error}");
            ExitCode::FAILURE
        }
    }
}
