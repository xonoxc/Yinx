use clap::Parser;

#[derive(Parser, Debug)]
#[command(name = "yinx", version, about = "A terminal HTTP client with streaming and workflow support")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(clap::Subcommand, Debug)]
enum Commands {
    /// Run a single request
    Run { url: Option<String> },
    /// Import a collection
    Import { file: String },
    /// Stream a response
    Stream { url: String },
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();
    let cli = Cli::parse();

    match cli.command {
        None | Some(Commands::Run { .. }) => {
            println!("yinx TUI mode (not yet implemented)");
        }
        Some(Commands::Import { .. }) => {
            println!("Import command (not yet implemented)");
        }
        Some(Commands::Stream { .. }) => {
            println!("Stream command (not yet implemented)");
        }
    }
}
