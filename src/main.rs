use clap::{Parser, Subcommand};
use dirs::config_dir;
use std::env;
use std::fs;
use std::path::PathBuf;
use tracing_appender::rolling;
use tracing_subscriber::{fmt, EnvFilter, layer::SubscriberExt};
use tracing_subscriber::layer::Layer;
use tracing_subscriber::util::SubscriberInitExt;

#[derive(Parser)]
#[command(name = "grounding", version, about = "Retrieval engine for LLM context")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

fn get_log_directory() -> Option<PathBuf> {
    // 1. Try GROUNDING_LOG_DIR override
    if let Ok(log_dir) = env::var("GROUNDING_LOG_DIR") {
        let path = PathBuf::from(log_dir);
        match fs::create_dir_all(&path) {
            Ok(()) => return Some(path),
            Err(e) => eprintln!("Warning: Failed to create GROUNDING_LOG_DIR {}: {}", path.display(), e),
        }
    }

    // 2. Try XDG config directory
    if let Some(mut config_dir) = config_dir() {
        config_dir.push("grounding");
        config_dir.push("logs");
        match fs::create_dir_all(&config_dir) {
            Ok(()) => return Some(config_dir),
            Err(e) => eprintln!("Warning: Failed to create config log directory {}: {}", config_dir.display(), e),
        }
    }

    // 3. Final fallback: HOME/.config/grounding/logs
    if let Ok(home) = env::var("HOME") {
        let mut path = PathBuf::from(home);
        path.push(".config");
        path.push("grounding");
        path.push("logs");
        match fs::create_dir_all(&path) {
            Ok(()) => return Some(path),
            Err(e) => eprintln!("Warning: Failed to create home log directory {}: {}", path.display(), e),
        }
    }

    None
}

#[derive(Subcommand)]
enum Commands {
    Serve {
        #[arg(long, default_value = "./data")]
        data_dir: String,
        #[arg(long, default_value_t = 8080)]
        port: u16,
    },
    Mcp {
        #[arg(long, default_value = "./data")]
        data_dir: String,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let log_dir = get_log_directory();

    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| "grounding=debug".into());

    let stdout_layer = fmt::layer()
        .with_writer(std::io::stdout)
        .with_filter(filter.clone());

    // Keep the guard alive for the entire main function
    let mut _guard = None;

    if let Some(dir) = log_dir {
        let file_appender = rolling::daily(dir, "grounding");
        let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);
        _guard = Some(guard);
        let file_layer = fmt::layer()
            .with_writer(non_blocking)
            .with_ansi(false)
            .with_filter(filter);
        tracing_subscriber::registry()
            .with(stdout_layer)
            .with(file_layer)
            .init();
    } else {
        tracing_subscriber::registry()
            .with(stdout_layer)
            .init();
    }

    let cli = Cli::parse();
    match cli.command {
        Commands::Serve { data_dir, port } => {
            let data_dir = std::env::var("GROUNDING_DATA_DIR").unwrap_or(data_dir);
            let port = std::env::var("GROUNDING_PORT")
                .ok()
                .and_then(|v| v.parse::<u16>().ok())
                .unwrap_or(port);
            grounding::serve(&data_dir, port).await?;
        }
        Commands::Mcp { data_dir } => {
            let data_dir = std::env::var("GROUNDING_DATA_DIR").unwrap_or(data_dir);
            grounding::serve_mcp_stdio(&data_dir).await?;
        }
    }
    Ok(())
}