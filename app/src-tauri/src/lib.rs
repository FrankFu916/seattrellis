//! SeatTrellis desktop shell (Tauri v0).
//!
//! The shell starts the loopback Rust backend (`seattrellis_app::server`) on an
//! OS-assigned free port and opens a WebView window pointed at it. The React
//! workbench talks to the backend over plain loopback HTTP — the same origin
//! and endpoints as the browser workspace — with no Python and no Node.

use std::thread;

use seattrellis_app::server::{resolve_web_root, Server, ServerConfig};
use tauri::{WebviewUrl, WebviewWindowBuilder};

/// Bind the backend, spawn its accept loop on a background thread, then open
/// the main window at the backend URL. Exits with a message on setup failure.
pub fn run() {
    let web_root = match resolve_web_root() {
        Ok(root) => root,
        Err(error) => {
            eprintln!("[seattrellis] {error}");
            std::process::exit(1);
        }
    };

    // Port 0 asks the OS for a free port, so the shell never collides with an
    // already-running workspace or a second app instance.
    let config = ServerConfig::new(0, web_root);
    let server = match Server::bind(&config) {
        Ok(server) => server,
        Err(error) => {
            eprintln!("[seattrellis] could not start backend: {error}");
            std::process::exit(1);
        }
    };
    let backend_url = format!("http://{}/", server.addr());
    eprintln!("[seattrellis] backend at {backend_url}");

    let _ = thread::Builder::new()
        .name("seattrellis-backend".to_string())
        .spawn(move || {
            if let Err(error) = server.serve() {
                eprintln!("[seattrellis] backend error: {error}");
            }
        });

    match tauri::Builder::default()
        .setup(move |app| {
            let external = match backend_url.parse() {
                Ok(url) => url,
                Err(error) => return Err(format!("invalid backend URL: {error}").into()),
            };
            WebviewWindowBuilder::new(app, "main", WebviewUrl::External(external))
                .title("SeatTrellis")
                .inner_size(1280.0, 800.0)
                .build()?;
            Ok(())
        })
        .run(tauri::generate_context!())
    {
        Ok(()) => {}
        Err(error) => {
            eprintln!("[seattrellis] shell error: {error}");
            std::process::exit(1);
        }
    }
}
