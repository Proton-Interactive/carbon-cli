mod state;
mod sourcemap;

use clap::{Parser, Subcommand};
use state::{AppState, SyncCommand};
use tracing::{error, info, Level};
use tracing_subscriber::FmtSubscriber;
use axum::{
    extract::State,
    routing::{get, post},
    Json, Router,
};

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Starts the sync server (default)
    Serve {
        #[arg(short, long, default_value_t = 8000)]
        port: u16,
    },
    /// Import scripts from Roblox
    Import,
    /// Export scripts to Roblox
    Export,
    /// Generate sourcemap for Luau LSP
    Sourcemap,
    /// Start LSP server
    Lsp,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .finish();

    tracing::subscriber::set_global_default(subscriber)
        .expect("setting default subscriber failed");

    let cli = Cli::parse();

    // Default port
    let port = 8000;

    match &cli.command {
        Some(Commands::Serve { port }) => {
            info!("Starting server on port {}", port);
            server(*port).await?;
        }
        Some(Commands::Import) => {
            info!("Triggering Import...");
            send_command_to_server(SyncCommand::Import, port).await?;
        }
        Some(Commands::Export) => {
            info!("Triggering Export...");
            send_command_to_server(SyncCommand::Export, port).await?;
        }
        Some(Commands::Sourcemap) => {
            info!("Generating sourcemap...");
            let cwd = std::env::current_dir()?;
            let json = sourcemap::generate_sourcemap(cwd)?;
            println!("{}", json);
        }
        Some(Commands::Lsp) => {
            info!("Starting LSP...");
            // In a real implementation, this would start the language server loop.
            // For now, we just keep the process alive so Zed doesn't think it crashed.
            println!("Carbon LSP started (placeholder)");
            std::future::pending::<()>().await;
        }
        None => {
            // Default to serve if no command provided
            info!("Starting server on default port {}", port);
            server(port).await?;
        }
    }

    Ok(())
}

async fn server(port: u16) -> anyhow::Result<()> {
    use std::net::SocketAddr;

    let state = AppState::new();

    let app = Router::new()
        .route("/", get(root))
        .route("/poll", get(poll_command))
        .route("/command", post(receive_command))
        // Placeholder for actual sync endpoints
        .route("/sync/update", post(sync_update))
        .with_state(state);

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    info!("Listening on {}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

// --- Handlers ---

async fn root() -> Json<serde_json::Value> {
    Json(serde_json::json!({ "message": "Carbon Core Running" }))
}

/// Endpoint for the Roblox plugin to poll for pending commands (Import/Export)
async fn poll_command(State(state): State<AppState>) -> Json<serde_json::Value> {
    let cmd = state.pop_command();
    // Returns { "command": "import" } or { "command": null }
    Json(serde_json::json!({ "command": cmd }))
}

/// Endpoint for CLI/Zed to trigger commands
async fn receive_command(
    State(state): State<AppState>,
    Json(payload): Json<SyncCommand>,
) -> Json<serde_json::Value> {
    info!("Received command request: {:?}", payload);
    state.set_command(payload);
    Json(serde_json::json!({ "success": true }))
}

/// Receives file updates from Roblox and writes them to disk
async fn sync_update(Json(payload): Json<serde_json::Value>) -> Json<serde_json::Value> {
    info!("Received sync update from Roblox");

    if let Some(files) = payload.get("files").and_then(|f| f.as_array()) {
        for file in files {
            if let (Some(path_str), Some(content)) = (
                file.get("path").and_then(|p| p.as_str()),
                file.get("content").and_then(|c| c.as_str()),
            ) {
                // Basic safety check
                if path_str.contains("..") {
                    error!("Skipping unsafe path: {}", path_str);
                    continue;
                }

                let path = std::path::Path::new(path_str);

                if let Some(parent) = path.parent() {
                    if let Err(e) = std::fs::create_dir_all(parent) {
                        error!("Failed to create directory {:?}: {}", parent, e);
                        continue;
                    }
                }

                if let Err(e) = std::fs::write(path, content) {
                    error!("Failed to write file {:?}: {}", path, e);
                } else {
                    info!("Imported: {:?}", path);
                }
            }
        }

        // Auto-generate sourcemap
        if let Ok(cwd) = std::env::current_dir() {
            match sourcemap::generate_sourcemap(cwd) {
                Ok(json) => {
                    if let Err(e) = std::fs::write("sourcemap.json", json) {
                        error!("Failed to write sourcemap.json: {}", e);
                    } else {
                        info!("Generated sourcemap.json");
                    }
                }
                Err(e) => error!("Failed to generate sourcemap: {}", e),
            }
        }
    }

    Json(serde_json::json!({ "success": true }))
}

// --- Client Helper ---

/// Sends a command to the running server via HTTP
async fn send_command_to_server(cmd: SyncCommand, port: u16) -> anyhow::Result<()> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpStream;

    let body = serde_json::to_string(&cmd)?;
    let len = body.len();

    // Construct a minimal HTTP POST request
    let request = format!(
        "POST /command HTTP/1.1\r\n\
         Host: 127.0.0.1:{}\r\n\
         Content-Type: application/json\r\n\
         Content-Length: {}\r\n\
         Connection: close\r\n\
         \r\n\
         {}",
        port, len, body
    );

    let addr = format!("127.0.0.1:{}", port);

    match TcpStream::connect(&addr).await {
        Ok(mut stream) => {
            stream.write_all(request.as_bytes()).await?;

            let mut response = String::new();
            // Read response (simplified)
            stream.read_to_string(&mut response).await?;

            if response.contains("200 OK") {
                info!("Command sent successfully.");
            } else {
                error!("Server returned error or invalid response.");
                println!("Response: {}", response);
            }
        }
        Err(e) => {
            error!("Failed to connect to server at {}. Is it running?", addr);
            return Err(e.into());
        }
    }

    Ok(())
}
