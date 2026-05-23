use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "grounding", version, about = "Retrieval engine for LLM context")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Serve {
        #[arg(long, default_value = "./data")]
        data_dir: String,
        #[arg(long, default_value_t = 8080)]
        port: u16,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "grounding=info".into()),
        )
        .init();

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
    }
    Ok(())
}
