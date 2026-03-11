use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(name = "flowctl", version, about = "CLI for flowd")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Debug, Subcommand)]
enum Commands {
    Status,
    Patterns,
    Suggest,
    Tail,
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Some(Commands::Status) => println!("flowd status: template skeleton"),
        Some(Commands::Patterns) => println!("patterns: not implemented"),
        Some(Commands::Suggest) => println!("suggestions: not implemented"),
        Some(Commands::Tail) => println!("tail: not implemented"),
        None => println!("Use --help to see available commands."),
    }
}
