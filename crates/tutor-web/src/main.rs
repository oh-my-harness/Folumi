#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::PathBuf;

use axum::extract::DefaultBodyLimit;
use tower_http::cors::{Any, CorsLayer};

mod knowledge_runtime;
mod knowledge_store;
mod memory_store;
mod memory_tool;
mod notebook_store;
mod quiz_store;
mod quiz_tool;
mod research_tool;
mod routes;
mod session;
mod settings_store;
mod space_tool;
mod stream;
mod tutor_memory_store;
mod tutor_memory_tool;
mod tutor_store;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = ServerConfig::from_args(std::env::args().skip(1))?;
    if config.exit_on_stdin_close {
        spawn_stdin_close_watchdog();
    }
    std::fs::create_dir_all(&config.data_dir)?;
    let pool = session::SessionPool::new_with_root(config.data_dir.join("sessions"));
    let knowledge = knowledge_store::KnowledgeStore::new_with_path(
        config.data_dir.join("knowledge-bases.json"),
    );
    let quizzes = std::sync::Arc::new(quiz_store::QuizStore::new_with_path(
        config.data_dir.join("quizzes.json"),
    ));
    let notebook = std::sync::Arc::new(notebook_store::NotebookStore::new_with_path(
        config.data_dir.join("notebook"),
    ));
    if let Err(error) = notebook.start_watcher() {
        eprintln!("failed to start notebook vault watcher: {error}");
    }
    let memory = std::sync::Arc::new(memory_store::MemoryStore::new_with_root(
        config.data_dir.join("memory"),
    ));
    let settings = std::sync::Arc::new(settings_store::SettingsStore::new_with_path(
        config.data_dir.join("settings.json"),
    ));
    let tutors = std::sync::Arc::new(tutor_store::TutorStore::new_with_root(
        config.data_dir.join("tutors"),
    ));
    let tutor_memory = std::sync::Arc::new(tutor_memory_store::TutorMemoryStore::new_with_root(
        config.data_dir.join("tutors"),
    ));
    let rag_root = config.data_dir.join("rag");

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let app = axum::Router::new()
        .merge(routes::knowledge::knowledge_router(
            knowledge.clone(),
            memory.clone(),
            rag_root.clone(),
        ))
        .merge(routes::quiz::quiz_router(
            quizzes.clone(),
            knowledge.clone(),
            notebook.clone(),
            memory.clone(),
            rag_root.clone(),
            config.data_dir.join("workflow-sessions").join("quiz"),
        ))
        .merge(routes::notebook::notebook_router(
            notebook.clone(),
            memory.clone(),
        ))
        .merge(routes::space::space_router(
            notebook.clone(),
            quizzes.clone(),
        ))
        .merge(routes::memory::memory_router(
            memory.clone(),
            config.data_dir.join("workflow-sessions").join("memory"),
        ))
        .merge(routes::settings::settings_router(settings.clone()))
        .merge(routes::tutors::tutors_router(
            tutors.clone(),
            settings.clone(),
            tutor_memory.clone(),
        ))
        .merge(routes::sessions::sessions_router(
            pool.clone(),
            knowledge.clone(),
            tutors.clone(),
            settings.clone(),
        ))
        .merge(routes::ws::ws_router(
            pool.clone(),
            knowledge.clone(),
            memory.clone(),
            notebook.clone(),
            quizzes.clone(),
            routes::ws::TutorRuntimeStores::new(tutors.clone(), tutor_memory),
            rag_root,
        ))
        .layer(DefaultBodyLimit::max(64 * 1024 * 1024))
        .layer(cors);

    let addr = SocketAddr::new(config.host, config.port);
    println!("tutor-web listening on {addr}");

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ServerConfig {
    host: IpAddr,
    port: u16,
    data_dir: PathBuf,
    exit_on_stdin_close: bool,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: IpAddr::V4(Ipv4Addr::LOCALHOST),
            port: 8080,
            data_dir: default_data_dir(),
            exit_on_stdin_close: false,
        }
    }
}

impl ServerConfig {
    fn from_args(args: impl IntoIterator<Item = String>) -> anyhow::Result<Self> {
        let mut config = Self::default();
        let mut args = args.into_iter();

        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--host" => {
                    let value = args
                        .next()
                        .ok_or_else(|| anyhow::anyhow!("--host requires a value"))?;
                    config.host = value.parse()?;
                }
                "--port" => {
                    let value = args
                        .next()
                        .ok_or_else(|| anyhow::anyhow!("--port requires a value"))?;
                    config.port = value.parse()?;
                }
                "--data-dir" => {
                    let value = args
                        .next()
                        .ok_or_else(|| anyhow::anyhow!("--data-dir requires a value"))?;
                    config.data_dir = PathBuf::from(value);
                }
                "--exit-on-stdin-close" => {
                    config.exit_on_stdin_close = true;
                }
                "--help" | "-h" => {
                    println!(
                        "Usage: tutor-web [--host 127.0.0.1] [--port 8080] [--data-dir .llm-tutor] [--exit-on-stdin-close]"
                    );
                    std::process::exit(0);
                }
                unknown => anyhow::bail!("unknown argument: {unknown}"),
            }
        }

        Ok(config)
    }
}

fn spawn_stdin_close_watchdog() {
    std::thread::spawn(|| {
        wait_for_reader_close(std::io::stdin());
        std::process::exit(0);
    });
}

fn wait_for_reader_close(mut reader: impl std::io::Read) {
    let mut buffer = [0_u8; 1];
    loop {
        match reader.read(&mut buffer) {
            Ok(0) => return,
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
            Err(_) => return,
        }
    }
}

fn default_data_dir() -> PathBuf {
    if let Ok(value) = std::env::var("LLM_TUTOR_HOME") {
        let path = PathBuf::from(value);
        if !path.as_os_str().is_empty() {
            return path;
        }
    }

    std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join(".llm-tutor")
}

#[cfg(test)]
mod tests {
    use super::{ServerConfig, default_data_dir, wait_for_reader_close};
    use std::net::{IpAddr, Ipv4Addr};

    #[test]
    fn parses_host_and_port() {
        let config = ServerConfig::from_args([
            "--host".to_string(),
            "127.0.0.1".to_string(),
            "--port".to_string(),
            "43127".to_string(),
        ])
        .unwrap();

        assert_eq!(
            config,
            ServerConfig {
                host: IpAddr::V4(Ipv4Addr::LOCALHOST),
                port: 43127,
                data_dir: default_data_dir(),
                exit_on_stdin_close: false,
            }
        );
    }

    #[test]
    fn parses_data_dir() {
        let config = ServerConfig::from_args([
            "--data-dir".to_string(),
            "D:/tmp/llm-tutor-data".to_string(),
        ])
        .unwrap();

        assert_eq!(
            config.data_dir,
            std::path::PathBuf::from("D:/tmp/llm-tutor-data")
        );
    }

    #[test]
    fn parses_exit_on_stdin_close() {
        let config = ServerConfig::from_args(["--exit-on-stdin-close".to_string()]).unwrap();

        assert!(config.exit_on_stdin_close);
    }

    #[test]
    fn reader_close_waits_until_eof() {
        wait_for_reader_close(std::io::Cursor::new(b"parent pipe data"));
    }
}
