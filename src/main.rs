use std::path::PathBuf;

use clap::{Parser, Subcommand};
use tracing::info;

use meeting_notes_daemon::filesdb::FilesDb;
use meeting_notes_daemon::people::PeopleManager;
use meeting_notes_daemon::server;
use meeting_notes_daemon::session::SessionManager;
use meeting_notes_daemon::settings::AppSettings;

fn install_signal_handlers() {
    unsafe {
        for sig in [libc::SIGSEGV, libc::SIGBUS, libc::SIGABRT] {
            libc::signal(sig, crash_handler as libc::sighandler_t);
        }
    }
}

extern "C" fn crash_handler(sig: libc::c_int) {
    let name = match sig {
        libc::SIGSEGV => "SIGSEGV (segmentation fault)",
        libc::SIGBUS => "SIGBUS (bus error)",
        libc::SIGABRT => "SIGABRT (abort)",
        _ => "unknown signal",
    };
    eprintln!("\n=== FATAL: {} (signal {}) ===", name, sig);
    eprintln!("Set RUST_BACKTRACE=1 for a backtrace.");
    eprintln!("{:?}", std::backtrace::Backtrace::force_capture());
    unsafe {
        libc::signal(sig, libc::SIG_DFL);
        libc::raise(sig);
    }
}

const APP_NAME: &str = "org.rankun.meeting-notes";

fn default_data_dir() -> PathBuf {
    dirs::home_dir()
        .expect("could not determine home directory")
        .join(".local/share")
        .join(APP_NAME)
}

#[derive(Parser)]
#[command(name = "meeting-notes-daemon")]
#[command(about = "System-level audio recorder and meeting notes processor")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Start the HTTP API server
    Serve {
        /// Port to listen on
        #[arg(short, long, default_value = "33487")]
        port: u16,

        /// Host to bind to
        #[arg(long, default_value = "127.0.0.1")]
        host: String,

        /// Data directory for recordings and app data
        #[arg(short = 'd', long)]
        data_dir: Option<PathBuf>,

        /// Enable built-in web UI
        #[arg(long)]
        web_ui: bool,
    },
}

#[tokio::main]
async fn main() {
    install_signal_handlers();

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "meeting_notes_daemon=info".into()),
        )
        .init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Serve { port, host, data_dir, web_ui } => {
            info!("Meeting Notes daemon starting...");
            let data_dir = data_dir.unwrap_or_else(default_data_dir);
            let recordings_dir = data_dir.join("recordings");
            std::fs::create_dir_all(&recordings_dir)
                .expect("failed to create recordings directory");

            let data_dir = std::fs::canonicalize(&data_dir).unwrap_or(data_dir);
            let recordings_dir = std::fs::canonicalize(&recordings_dir).unwrap_or(recordings_dir);

            let manager = SessionManager::new(recordings_dir.clone());
            manager.load_from_disk().await;
            manager.start_file_size_ticker();

            let people_manager = PeopleManager::new(&data_dir);
            people_manager.load_from_disk().await;

            let files_db = FilesDb::load_from_disk(&recordings_dir).await;

            let settings = AppSettings::load_or_create(&data_dir);
            let shared_settings = std::sync::Arc::new(tokio::sync::RwLock::new(settings));

            let app = server::create_router(
                manager, people_manager, shared_settings, files_db, web_ui,
            );

            let addr = format!("{}:{}", host, port);
            let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
            info!("Server listening on http://{}", addr);
            info!("Data directory: \"{}\"", data_dir.display());
            info!("Recordings directory: \"{}\"", recordings_dir.display());
            if web_ui {
                info!("Web UI available at http://{}", addr);
            }

            axum::serve(listener, app).await.unwrap();
        }
    }
}
