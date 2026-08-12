//! SeatTrellis desktop shell.
//!
//! The shell starts the loopback Rust backend (`seattrellis_app::server`) on an
//! OS-assigned free port and opens a WebView window pointed at it. The React
//! workbench talks to the backend over plain loopback HTTP — the same origin
//! and endpoints as the browser workspace — with no Python and no Node.
//!
//! The two IPC commands bridge the D14 native file dialogs: the OS dialog
//! returns a path the user explicitly chose, and `read_user_file` /
//! `write_user_file` move the bytes between the WebView and the local disk.
//! They are the only bridge surface; everything else stays on loopback HTTP.

use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use std::thread;

use seattrellis_server::server::{resolve_web_root, Server, ServerConfig};
use tauri::{WebviewUrl, WebviewWindowBuilder};

/// Upper bound for roster/export files moved through the native dialogs.
const MAX_BRIDGE_FILE_BYTES: u64 = 8 * 1024 * 1024;

/// Read a file the user picked in a native dialog (PD-D14 entry ②).
///
/// The path comes from `tauri-plugin-dialog`'s `open()`, so the user chose
/// it through the OS file picker — the same trust boundary as the dialog
/// itself. The manual-path entry of the picker never calls this command
/// with an absolute path: it goes through the backend's trusted-root
/// endpoint instead (absolute paths are rejected there).
#[tauri::command]
fn read_user_file(path: String) -> Result<Vec<u8>, String> {
    let metadata = std::fs::metadata(&path).map_err(|error| format!("cannot stat file: {error}"))?;
    if !metadata.is_file() {
        return Err("path is not a file".to_string());
    }
    if metadata.len() > MAX_BRIDGE_FILE_BYTES {
        return Err("file is too large".to_string());
    }
    std::fs::read(&path).map_err(|error| format!("cannot read file: {error}"))
}

/// Write export bytes to a path the user chose in a native save dialog
/// (PD-D14 entry ②, save side). Overwrites only what the dialog allowed.
#[tauri::command]
fn write_user_file(path: String, content: Vec<u8>) -> Result<(), String> {
    if content.len() as u64 > MAX_BRIDGE_FILE_BYTES {
        return Err("file is too large".to_string());
    }
    std::fs::write(&path, content).map_err(|error| format!("cannot write file: {error}"))
}

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
    // already-running workspace or a second app instance. `SEATTRELLIS_PORT`
    // overrides it for the dev loop, where the vite dev server proxies /api
    // to a known backend port.
    let port = std::env::var("SEATTRELLIS_PORT")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(0);
    // PD-D14: typed paths resolve against a pinned trusted root. Launched
    // from a Finder double-click the cwd is `/`, which would make the
    // containment check vacuous; fall back to the user's home in that case.
    let trusted_root = std::env::current_dir()
        .ok()
        .filter(|dir| dir != std::path::Path::new("/"))
        .unwrap_or_else(|| {
            std::env::var_os("HOME")
                .map(std::path::PathBuf::from)
                .unwrap_or_else(|| std::path::PathBuf::from("/"))
        });
    let config = ServerConfig::new(port, web_root).with_trusted_root(trusted_root);
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
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![read_user_file, write_user_file])
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
