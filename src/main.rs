use std::path::PathBuf;

use clap::{Parser, Subcommand};
use tracing::info;

use meeting_notes_daemon::chat::manager::ConversationManager;
use meeting_notes_daemon::filesdb::FilesDb;
use meeting_notes_daemon::llm::secrets::LlmSecrets;
use meeting_notes_daemon::people::PeopleManager;
use meeting_notes_daemon::server;
use meeting_notes_daemon::session::SessionManager;
use meeting_notes_daemon::settings::AppSettings;
use meeting_notes_daemon::tags::TagsManager;

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
            info!("Meeting Notes daemon starting on port {}...", port);
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

            let tags_manager = TagsManager::new(&data_dir);
            tags_manager.load_from_disk().await;

            let files_db = FilesDb::load_from_disk(&recordings_dir).await;

            let settings = AppSettings::load_or_create(&data_dir);
            let shared_settings = std::sync::Arc::new(tokio::sync::RwLock::new(settings));

            let llm_secrets = LlmSecrets::load_or_create(&data_dir);
            let shared_secrets = std::sync::Arc::new(tokio::sync::RwLock::new(llm_secrets));

            let conversation_manager = ConversationManager::new(&data_dir);

            // Generate CLAUDE.md and markdown index files
            {
                let self_intro = shared_settings.read().await.chat_self_intro.clone();
                meeting_notes_daemon::markdown::write_claude_md(&data_dir, self_intro.as_deref());
            }
            {
                use meeting_notes_daemon::markdown;
                let mut sessions = manager.session_entries().await;
                let mut people = people_manager.person_entries().await;
                let people_dir = people_manager.people_dir().to_path_buf();
                let rec_dir = recordings_dir.clone();
                let (rec_index_bytes, people_index_bytes) =
                    tokio::task::spawn_blocking(move || {
                        let r = markdown::write_recordings_index(&rec_dir, &mut sessions);
                        let p = markdown::write_people_index(&people_dir, &mut people);
                        (r, p)
                    }).await.unwrap();
                info!(
                    "Updated markdown indexes: recordings/index.md ({}), people/index.md ({})",
                    markdown::human_size(rec_index_bytes),
                    markdown::human_size(people_index_bytes),
                );
            }

            // Resume any pending extraction jobs from before restart
            meeting_notes_daemon::services::transcripts::resume_pending_extractions(
                manager.clone(), people_manager.clone(),
                files_db.clone(), shared_settings.clone(),
                shared_secrets.clone(), tags_manager.clone(),
            ).await;

            let claude_runner = meeting_notes_daemon::llm::claude_code::ClaudeCodeRunner::new(&data_dir);

            let app = server::create_router(
                manager, people_manager, shared_settings, files_db, tags_manager,
                conversation_manager, shared_secrets, claude_runner, web_ui,
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
