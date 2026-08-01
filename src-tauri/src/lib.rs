#[cfg(debug_assertions)]
use std::process::{Child, Stdio};
use std::{
    net::{TcpListener, TcpStream},
    path::{Path, PathBuf},
    process::Command,
    sync::Mutex,
    thread,
    time::{Duration, Instant},
};

use tauri::Manager;
#[cfg(not(debug_assertions))]
use tauri_plugin_shell::ShellExt;
#[cfg(not(debug_assertions))]
use tauri_plugin_shell::process::CommandChild;

#[cfg(target_os = "windows")]
mod native_window;

struct BackendState {
    url: String,
    data_dir: PathBuf,
    child: Mutex<Option<BackendProcess>>,
}

impl BackendState {
    fn shutdown(&self) {
        if let Ok(mut child) = self.child.lock()
            && let Some(child) = child.take()
        {
            child.kill();
        }
    }
}

enum BackendProcess {
    #[cfg(debug_assertions)]
    Std(Child),
    #[cfg(not(debug_assertions))]
    Sidecar(CommandChild),
}

impl Drop for BackendState {
    fn drop(&mut self) {
        self.shutdown();
    }
}

impl BackendProcess {
    fn kill(self) {
        match self {
            #[cfg(debug_assertions)]
            Self::Std(mut child) => {
                let _ = child.kill();
                let _ = child.wait();
            }
            #[cfg(not(debug_assertions))]
            Self::Sidecar(child) => {
                let _ = child.kill();
            }
        }
    }

    fn try_wait_status(&mut self) -> std::io::Result<Option<String>> {
        match self {
            #[cfg(debug_assertions)]
            Self::Std(child) => Ok(child.try_wait()?.map(|status| status.to_string())),
            #[cfg(not(debug_assertions))]
            Self::Sidecar(_) => Ok(None),
        }
    }
}

#[tauri::command]
fn get_backend_url(state: tauri::State<'_, BackendState>) -> String {
    state.url.clone()
}

#[tauri::command]
fn get_data_dir(state: tauri::State<'_, BackendState>) -> String {
    state.data_dir.to_string_lossy().to_string()
}

#[tauri::command]
fn open_data_dir(state: tauri::State<'_, BackendState>) -> Result<(), String> {
    std::fs::create_dir_all(&state.data_dir).map_err(|error| error.to_string())?;
    open_directory(&state.data_dir).map_err(|error| error.to_string())
}

#[tauri::command]
fn open_external_url(url: String) -> Result<(), String> {
    if !is_allowed_external_url(&url) {
        return Err("unsupported external URL".into());
    }
    open_url(&url).map_err(|error| error.to_string())
}

#[tauri::command]
fn read_clipboard_text() -> Result<String, String> {
    let mut clipboard = arboard::Clipboard::new().map_err(|error| error.to_string())?;
    clipboard.get_text().map_err(|error| error.to_string())
}

#[tauri::command]
fn write_clipboard_text(text: String) -> Result<(), String> {
    let mut clipboard = arboard::Clipboard::new().map_err(|error| error.to_string())?;
    clipboard.set_text(text).map_err(|error| error.to_string())
}

#[tauri::command]
fn set_native_window_theme(window: tauri::WebviewWindow, theme: String) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    native_window::apply_theme(&window, &theme)?;

    #[cfg(not(target_os = "windows"))]
    let _ = (window, theme);

    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_shell::init())
        .invoke_handler(tauri::generate_handler![
            get_backend_url,
            get_data_dir,
            open_data_dir,
            open_external_url,
            read_clipboard_text,
            write_clipboard_text,
            set_native_window_theme
        ])
        .setup(|app| {
            #[cfg(target_os = "windows")]
            if let Some(window) = app.get_webview_window("main") {
                native_window::configure(&window)?;
            }

            let port = find_free_port()?;
            let url = format!("http://127.0.0.1:{port}");
            let data_dir = app_data_dir(app)?;
            std::fs::create_dir_all(&data_dir)?;
            let child = spawn_backend(app, port, &data_dir)?;
            let child = wait_for_backend(port, child)?;
            app.manage(BackendState {
                url,
                data_dir,
                child: Mutex::new(Some(child)),
            });
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("failed to build Folumi desktop app");

    app.run(|app_handle, event| {
        if matches!(event, tauri::RunEvent::Exit)
            && let Some(state) = app_handle.try_state::<BackendState>()
        {
            state.shutdown();
        }
    });
}

fn find_free_port() -> std::io::Result<u16> {
    let listener = TcpListener::bind(("127.0.0.1", 0))?;
    Ok(listener.local_addr()?.port())
}

#[cfg(debug_assertions)]
fn spawn_backend(
    _app: &tauri::App,
    port: u16,
    data_dir: &std::path::Path,
) -> std::io::Result<BackendProcess> {
    let port = port.to_string();
    let data_dir = data_dir.to_string_lossy().to_string();
    let mut command = debug_backend_command()?;
    command
        .args([
            "--host",
            "127.0.0.1",
            "--port",
            &port,
            "--data-dir",
            &data_dir,
            "--exit-on-stdin-close",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());

    command.spawn().map(BackendProcess::Std)
}

#[cfg(not(debug_assertions))]
fn spawn_backend(
    app: &tauri::App,
    port: u16,
    data_dir: &std::path::Path,
) -> std::io::Result<BackendProcess> {
    let port = port.to_string();
    let data_dir = data_dir.to_string_lossy().to_string();
    let (_events, child) = app
        .shell()
        .sidecar("tutor-web")
        .map_err(io_error)?
        .args([
            "--host",
            "127.0.0.1",
            "--port",
            &port,
            "--data-dir",
            &data_dir,
            "--exit-on-stdin-close",
        ])
        .spawn()
        .map_err(io_error)?;

    Ok(BackendProcess::Sidecar(child))
}

fn wait_for_backend(port: u16, mut child: BackendProcess) -> std::io::Result<BackendProcess> {
    let timeout = std::env::var("LLM_TUTOR_BACKEND_STARTUP_TIMEOUT_SECS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .map(Duration::from_secs)
        .unwrap_or_else(|| Duration::from_secs(300));
    let deadline = Instant::now() + timeout;

    loop {
        if TcpStream::connect(("127.0.0.1", port)).is_ok() {
            return Ok(child);
        }

        if let Some(status) = child.try_wait_status()? {
            return Err(std::io::Error::other(format!(
                "tutor-web exited before it became ready: {status}"
            )));
        }

        if Instant::now() >= deadline {
            child.kill();
            return Err(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                format!(
                    "timed out waiting for tutor-web to start after {} seconds",
                    timeout.as_secs()
                ),
            ));
        }

        thread::sleep(Duration::from_millis(100));
    }
}

fn app_data_dir(app: &tauri::App) -> std::io::Result<PathBuf> {
    app.path().app_data_dir().map_err(io_error)
}

fn open_directory(path: &Path) -> std::io::Result<()> {
    #[cfg(target_os = "windows")]
    {
        Command::new("explorer").arg(path).spawn()?;
    }

    #[cfg(target_os = "macos")]
    {
        Command::new("open").arg(path).spawn()?;
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        Command::new("xdg-open").arg(path).spawn()?;
    }

    Ok(())
}

fn open_url(url: &str) -> std::io::Result<()> {
    #[cfg(target_os = "windows")]
    {
        Command::new("explorer").arg(url).spawn()?;
    }

    #[cfg(target_os = "macos")]
    {
        Command::new("open").arg(url).spawn()?;
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        Command::new("xdg-open").arg(url).spawn()?;
    }

    Ok(())
}

fn is_allowed_external_url(url: &str) -> bool {
    let lower = url.trim().to_ascii_lowercase();
    lower.starts_with("http://") || lower.starts_with("https://") || lower.starts_with("mailto:")
}

#[cfg(debug_assertions)]
fn debug_backend_command() -> std::io::Result<Command> {
    if let Ok(path) = std::env::var("LLM_TUTOR_BACKEND_BIN") {
        return Ok(Command::new(path));
    }

    let root =
        workspace_root().ok_or_else(|| std::io::Error::other("workspace root is unavailable"))?;
    let status = Command::new("cargo")
        .args(["build", "-p", "tutor-web", "--bin", "tutor-web"])
        .current_dir(&root)
        .status()?;
    if !status.success() {
        return Err(std::io::Error::other(format!(
            "failed to build tutor-web before startup: {status}"
        )));
    }

    let path = debug_backend_binary()
        .ok_or_else(|| std::io::Error::other("debug tutor-web path is unavailable"))?;
    if !path.exists() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!(
                "debug tutor-web binary was not produced at {}",
                path.display()
            ),
        ));
    }
    Ok(Command::new(path))
}

#[cfg(debug_assertions)]
fn debug_backend_binary() -> Option<PathBuf> {
    let binary_name = if cfg!(windows) {
        "tutor-web.exe"
    } else {
        "tutor-web"
    };
    workspace_root().map(|root| root.join("target").join("debug").join(binary_name))
}

#[cfg(debug_assertions)]
fn workspace_root() -> Option<PathBuf> {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .map(PathBuf::from)
}

fn io_error(error: impl std::error::Error + Send + Sync + 'static) -> std::io::Error {
    std::io::Error::other(error)
}
