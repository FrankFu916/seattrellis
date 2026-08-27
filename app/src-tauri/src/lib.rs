//! SeatTrellis desktop shell.
//!
//! The shell starts the loopback Rust backend (`seattrellis_web::server`) on an
//! OS-assigned free port and opens a WebView window pointed at it. The React
//! workbench talks to the backend over plain loopback HTTP — the same origin
//! and endpoints as the browser workspace — with no Python and no Node.
//!
//! The four IPC commands bridge the D14 native file dialogs: `pick_open_file`
//! and `pick_save_file` run the OS dialog inside the shell and register the
//! canonical chosen path in this process' grant registry; `read_user_file` /
//! `write_user_file` move bytes between the WebView and the local disk but
//! only for paths that were granted through a dialog. A compromised renderer
//! therefore cannot read or write arbitrary absolute paths (confused-deputy
//! fix): every byte-moving command re-canonicalizes the requested path and
//! rejects anything outside the registry.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use std::thread;

use seattrellis_server::server::{resolve_web_root, Server, ServerConfig};
use tauri::{State, WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_dialog::DialogExt;

/// Upper bound for roster/export files moved through the native dialogs.
const MAX_BRIDGE_FILE_BYTES: u64 = 8 * 1024 * 1024;

/// Production CSP for the workbench page. Mirrored in
/// `tauri.conf.json > app.security.csp` and injected into the external-URL
/// page by an initialization script. `unsafe-inline` styles are required by
/// React's inline `style` attributes; `ipc:` / `http://ipc.localhost` are the
/// Tauri IPC transports on Windows/Linux; everything else stays same-origin
/// to the loopback backend.
const WORKBENCH_CSP: &str = "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data: blob:; connect-src 'self' http://127.0.0.1:* http://localhost:* http://ipc.localhost ipc:; font-src 'self' data:";

/// Canonical paths granted through a native dialog in this session.
///
/// Only the dialog commands insert into this set, so membership proves the
/// path came from an explicit user choice inside the OS picker.
struct GrantedFiles(Mutex<HashSet<PathBuf>>);

/// Resolve a picked path to its canonical form for the grant registry.
///
/// A save-dialog target usually does not exist yet, so `canonicalize` fails;
/// in that case anchor on the canonical parent directory and keep the file
/// name. Both grant time and verify time use this function, so both sides of
/// the comparison see the same normalized form (macOS `/var` ↔ `/private/var`
/// aliasing included).
fn canonicalize_candidate(path: &Path) -> Result<PathBuf, String> {
    match path.canonicalize() {
        Ok(canonical) => Ok(canonical),
        Err(_) => {
            let parent = path
                .parent()
                .ok_or_else(|| "path has no parent directory".to_string())?;
            let name = path
                .file_name()
                .ok_or_else(|| "path has no file name".to_string())?;
            let canonical_parent = parent
                .canonicalize()
                .map_err(|_| "path was not granted through the file dialog".to_string())?;
            Ok(canonical_parent.join(name))
        }
    }
}

/// Check the requested path against the grant registry. Returns the canonical
/// path on success; any mismatch (never picked, renamed, deleted parent,
/// traversal alias) is rejected with one opaque message so the renderer learns
/// nothing about the filesystem beyond "denied".
fn ensure_granted(granted: &GrantedFiles, raw: &str) -> Result<PathBuf, String> {
    let canonical = canonicalize_candidate(Path::new(raw))?;
    if granted
        .0
        .lock()
        .map_err(|_| "grant registry unavailable".to_string())?
        .contains(&canonical)
    {
        Ok(canonical)
    } else {
        Err("path was not granted through the file dialog".to_string())
    }
}

/// Show the native open dialog (Rust side, PD-D14) and register the result.
///
/// Runs as an async command so `blocking_pick_file` never blocks the main
/// thread. Returns `None` when the user cancels. The renderer never chooses
/// the path itself — it can only read back what the dialog returned.
#[tauri::command]
async fn pick_open_file(
    app: tauri::AppHandle,
    granted: State<'_, GrantedFiles>,
    extensions: Option<Vec<String>>,
    label: Option<String>,
) -> Result<Option<String>, String> {
    eprintln!("[seattrellis] pick_open_file invoked");
    let mut dialog = app.dialog().file();
    let filters = extensions.unwrap_or_default();
    if !filters.is_empty() {
        let extension_refs: Vec<&str> = filters.iter().map(String::as_str).collect();
        dialog = dialog.add_filter(label.as_deref().unwrap_or("Files"), &extension_refs);
    }
    let picked =
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| dialog.blocking_pick_file()))
            .map_err(|panic| {
                let message = panic
                    .downcast_ref::<String>()
                    .cloned()
                    .or_else(|| panic.downcast_ref::<&str>().map(|s| s.to_string()))
                    .unwrap_or_else(|| "unknown dialog panic".to_string());
                eprintln!("[seattrellis] pick_open_file dialog panicked: {message}");
                format!("dialog failed: {message}")
            })?;
    let Some(picked) = picked else {
        return Ok(None);
    };
    let path = picked.into_path().map_err(|error| error.to_string())?;
    let canonical = canonicalize_candidate(&path)?;
    granted
        .0
        .lock()
        .map_err(|_| "grant registry unavailable".to_string())?
        .insert(canonical.clone());
    eprintln!("[seattrellis] pick_open_file granted: {canonical:?}");
    Ok(Some(canonical.to_string_lossy().into_owned()))
}

/// Show the native save dialog (Rust side, PD-D14, save side) and register
/// the result. Mirrors [`pick_open_file`]; the target file may not exist yet,
/// so the grant anchors on the canonical parent directory.
#[tauri::command]
async fn pick_save_file(
    app: tauri::AppHandle,
    granted: State<'_, GrantedFiles>,
    filename: Option<String>,
) -> Result<Option<String>, String> {
    let mut dialog = app.dialog().file();
    if let Some(name) = filename.filter(|name| !name.is_empty()) {
        dialog = dialog.set_file_name(name);
    }
    let picked = dialog.blocking_save_file();
    let Some(picked) = picked else {
        return Ok(None);
    };
    let path = picked.into_path().map_err(|error| error.to_string())?;
    let canonical = canonicalize_candidate(&path)?;
    granted
        .0
        .lock()
        .map_err(|_| "grant registry unavailable".to_string())?
        .insert(canonical.clone());
    Ok(Some(canonical.to_string_lossy().into_owned()))
}

/// Read a file the user picked in a native dialog (PD-D14 entry ②).
///
/// The path must have been granted by `pick_open_file` earlier in this
/// session; anything else is rejected before touching the disk.
#[tauri::command]
fn read_user_file(path: String, granted: State<'_, GrantedFiles>) -> Result<Vec<u8>, String> {
    let canonical = ensure_granted(&granted, &path)?;
    let metadata =
        std::fs::metadata(&canonical).map_err(|error| format!("cannot stat file: {error}"))?;
    if !metadata.is_file() {
        return Err("path is not a file".to_string());
    }
    if metadata.len() > MAX_BRIDGE_FILE_BYTES {
        return Err("file is too large".to_string());
    }
    std::fs::read(&canonical).map_err(|error| format!("cannot read file: {error}"))
}

/// Write export bytes to a path the user chose in a native save dialog
/// (PD-D14 entry ②, save side). Overwrites only what the dialog allowed.
#[tauri::command]
fn write_user_file(
    path: String,
    content: Vec<u8>,
    granted: State<'_, GrantedFiles>,
) -> Result<(), String> {
    if content.len() as u64 > MAX_BRIDGE_FILE_BYTES {
        return Err("file is too large".to_string());
    }
    let canonical = ensure_granted(&granted, &path)?;
    std::fs::write(&canonical, content).map_err(|error| format!("cannot write file: {error}"))
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
        .manage(GrantedFiles(Mutex::new(HashSet::new())))
        .invoke_handler(tauri::generate_handler![
            pick_open_file,
            pick_save_file,
            read_user_file,
            write_user_file
        ])
        .setup(move |app| {
            let external = match backend_url.parse() {
                Ok(url) => url,
                Err(error) => return Err(format!("invalid backend URL: {error}").into()),
            };
            WebviewWindowBuilder::new(app, "main", WebviewUrl::External(external))
                .title("SeatTrellis")
                .inner_size(1280.0, 800.0)
                // The workbench implements drag-and-drop import with HTML5
                // drag events. Tauri's own drag-drop handler (on by default)
                // intercepts file drops on macOS/Windows and the DOM events
                // never fire, so it must be disabled for the third PD-D14
                // entry to work in the desktop shell.
                .disable_drag_drop_handler()
                .initialization_script(format!(
                    "window.__SEATTRELLIS_SESSION__ = '{session_token}';"
                ))
                // Tauri's config-level CSP only applies to custom-protocol
                // assets, but this shell loads the loopback backend through an
                // external URL. Inject the production policy as a meta tag at
                // document start so the workbench page is actually constrained
                // (the policy mirrors `tauri.conf.json > app.security.csp`).
                .initialization_script(format!(
                    "try{{var m=document.createElement('meta');\
                    m.httpEquiv='Content-Security-Policy';\
                    m.content={WORKBENCH_CSP:?};\
                    (document.head||document.documentElement).appendChild(m);}}catch(e){{}}"
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
