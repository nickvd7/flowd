use anyhow::Context;
use clap::{Parser, Subcommand};
use flow_core::config::Config;
use flow_db::{migrations::run_migrations, repo::list_suggestions};
use rusqlite::Connection;

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
    if let Err(error) = run() {
        eprintln!("error: {error:#}");
        std::process::exit(1);
    }
}

fn run() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Some(Commands::Status) => println!("flowd status: template skeleton"),
        Some(Commands::Patterns) => println!("patterns: not implemented"),
        Some(Commands::Suggest) => render_suggestions()?,
        Some(Commands::Tail) => println!("tail: not implemented"),
        None => println!("Use --help to see available commands."),
    }

    Ok(())
}

fn render_suggestions() -> anyhow::Result<()> {
    let db_path = std::env::var("FLOWD_DB_PATH")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| Config::default().database_path);
    let conn = Connection::open(&db_path)
        .with_context(|| format!("failed to open database at {db_path}"))?;
    run_migrations(&conn).context("failed to run database migrations")?;
    let suggestions = list_suggestions(&conn).context("failed to read suggestions")?;

    if suggestions.is_empty() {
        println!("No suggestions stored.");
        return Ok(());
    }

    for suggestion in suggestions {
        println!("{}", suggestion.proposal_text);
        println!(
            "  pattern: {} | repeats: {} | avg duration: {} ms",
            suggestion.canonical_summary, suggestion.count, suggestion.avg_duration_ms
        );
    }

    Ok(())
}
