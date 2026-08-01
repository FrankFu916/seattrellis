//! SeatTrellis desktop backend entry point.
//!
//! v0 shell: starts the loopback-only HTTP backend, prints the URL, and (only
//! when asked) opens the system browser. A Tauri shell can call
//! `seattrellis_app::server` directly and load the printed URL in a WebView,
//! replacing the browser entirely.

use std::process::ExitCode;

use seattrellis_app::server::{resolve_web_root, Server, ServerConfig};

/// Default loopback port, matching the Python workspace convention.
const DEFAULT_PORT: u16 = 8765;

fn print_usage() {
    eprintln!(
        "SeatTrellis desktop backend (v{})
USAGE:
  seattrellis_app [OPTIONS]

OPTIONS:
  --port <PORT>          Bind to PORT (default {DEFAULT_PORT}, env SEATTRELLIS_PORT)
  --open-browser         Open the default browser after binding (macOS)
  --version, -V          Print version and exit
  --help, -h             Show this help",
        env!("CARGO_PKG_VERSION")
    );
}

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let mut port = DEFAULT_PORT;
    let mut open_browser = false;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--help" | "-h" => {
                print_usage();
                return ExitCode::SUCCESS;
            }
            "--version" | "-V" => {
                println!("seattrellis_app {}", env!("CARGO_PKG_VERSION"));
                return ExitCode::SUCCESS;
            }
            "--open-browser" => open_browser = true,
            "--no-open-browser" => open_browser = false,
            "--port" => {
                let Some(raw) = args.next() else {
                    eprintln!("[seattrellis] --port requires a value");
                    print_usage();
                    return ExitCode::FAILURE;
                };
                port = match raw.parse() {
                    Ok(value) if value != 0 => value,
                    _ => {
                        eprintln!("[seattrellis] invalid port: {raw}");
                        return ExitCode::FAILURE;
                    }
                };
            }
            flag if flag.starts_with("--port=") => {
                let raw = &flag["--port=".len()..];
                port = match raw.parse() {
                    Ok(value) if value != 0 => value,
                    _ => {
                        eprintln!("[seattrellis] invalid port: {raw}");
                        return ExitCode::FAILURE;
                    }
                };
            }
            unknown => {
                eprintln!("[seattrellis] unknown argument: {unknown}");
                print_usage();
                return ExitCode::FAILURE;
            }
        }
    }

    if let Ok(raw) = std::env::var("SEATTRELLIS_PORT") {
        match raw.parse::<u16>() {
            Ok(value) if value != 0 => port = value,
            _ => {
                eprintln!("[seattrellis] invalid SEATTRELLIS_PORT: {raw}");
                return ExitCode::FAILURE;
            }
        }
    }

    let web_root = match resolve_web_root() {
        Ok(root) => root,
        Err(error) => {
            eprintln!("[seattrellis] {error}");
            return ExitCode::FAILURE;
        }
    };

    let config = ServerConfig::new(port, web_root);
    let server = match Server::bind(&config) {
        Ok(server) => server,
        Err(error) => {
            eprintln!(
                "[seattrellis] failed to bind {}:{}: {error}",
                config.host, config.port
            );
            return ExitCode::FAILURE;
        }
    };

    let url = format!("http://{}/", server.addr());
    println!("[seattrellis] SeatTrellis desktop backend (v{})", env!("CARGO_PKG_VERSION"));
    println!("[seattrellis] serving workbench from {}", config.web_root.display());
    println!("[seattrellis] workbench URL: {url}");
    println!("[seattrellis] health:        {url}api/v1/health");
    println!("[seattrellis] solve:         POST {url}api/v1/classes/generate");

    if open_browser {
        match std::process::Command::new("open").arg(&url).spawn() {
            Ok(_) => println!("[seattrellis] opening browser"),
            Err(error) => eprintln!("[seattrellis] could not open browser: {error}"),
        }
    }

    println!("[seattrellis] listening on {} (Ctrl-C to quit)", server.addr());
    match server.serve() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("[seattrellis] server error: {error}");
            ExitCode::FAILURE
        }
    }
}
