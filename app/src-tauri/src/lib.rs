//! SeatTrellis desktop shell.
//!
//! The shell starts the loopback Rust backend (`seattrellis_app::server`) on an
//! OS-assigned free port and opens a WebView window pointed at it. The React
//! workbench talks to the backend over plain loopback HTTP — the same origin
//! and endpoints as the browser workspace — with no Python and no Node.

use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use std::thread;

use seattrellis_server::server::{resolve_web_root, Server, ServerConfig};
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

    // The backend thread is joined on app exit: the shell requests a graceful
    // shutdown (M1-04 exit gate: no residual port or thread after exit) and
    // waits for the accept loop to drain instead of dropping it mid-request.
    // M1-05: the 256-bit session token is injected into the WebView's JS
    // memory at page load (never the URL, logs or disk); the workbench sends
    // it as `Authorization: Bearer` on every /api/* call.
    let session_token = server.session_token().to_string();
    let shutdown_flag = server.shutdown_flag();
    let backend_thread = thread::Builder::new()
        .name("seattrellis-backend".to_string())
        .spawn(move || {
            if let Err(error) = server.serve() {
                eprintln!("[seattrellis] backend error: {error}");
            }
        })
        .expect("spawning the backend thread must not fail");

    let app = tauri::Builder::default()
        .setup(move |app| {
            let external = match backend_url.parse() {
                Ok(url) => url,
                Err(error) => return Err(format!("invalid backend URL: {error}").into()),
            };
            WebviewWindowBuilder::new(app, "main", WebviewUrl::External(external))
                .title("SeatTrellis")
                .inner_size(1280.0, 800.0)
                .initialization_script(format!(
                    "window.__SEATTRELLIS_SESSION__ = '{session_token}';"
                ))
                .build()?;
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building tauri application");

    // Keep the join handle behind a mutex so the exit handler can take it.
    let backend_handle = Arc::new(Mutex::new(Some(backend_thread)));

    // Tauri 2's `App::run` returns `()`; the event loop ends when the last
    // window closes or the shell exits.
    app.run(move |_app_handle, event| {
        if let tauri::RunEvent::ExitRequested { .. } = event {
            shutdown_flag.store(true, Ordering::Relaxed);
            if let Some(thread) = backend_handle
                .lock()
                .ok()
                .and_then(|mut guard| guard.take())
            {
                let _ = thread.join();
            }
        }
    });
}
