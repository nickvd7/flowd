use anyhow::Context;
use clap::{Parser, Subcommand};
use flow_core::config::Config;
use flow_db::{
    open_database,
    repo::{list_automations, list_patterns, list_recent_sessions, list_suggestions},
};
use flow_exec::{
    approve_suggestion, disable_automation, dry_run_automation, enable_automation,
    execute_automation, list_runs, undo_automation_run,
};
use std::fmt::Display;

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
    Suggestions,
    Sessions,
    Tail,
    Approve { suggestion_id: i64 },
    Automations,
    Disable { automation_id: i64 },
    Enable { automation_id: i64 },
    Run { automation_id: i64 },
    DryRun { automation_id: i64 },
    Runs,
    Undo { run_id: i64 },
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
        Some(Commands::Patterns) => render_patterns()?,
        Some(Commands::Suggest) => render_suggestions()?,
        Some(Commands::Suggestions) => render_suggestions_table()?,
        Some(Commands::Sessions) => render_sessions()?,
        Some(Commands::Tail) => println!("tail: not implemented"),
        Some(Commands::Approve { suggestion_id }) => approve_automation_command(suggestion_id)?,
        Some(Commands::Automations) => render_automations()?,
        Some(Commands::Disable { automation_id }) => disable_automation_command(automation_id)?,
        Some(Commands::Enable { automation_id }) => enable_automation_command(automation_id)?,
        Some(Commands::Run { automation_id }) => run_automation_command(automation_id)?,
        Some(Commands::DryRun { automation_id }) => dry_run_automation_command(automation_id)?,
        Some(Commands::Runs) => render_runs()?,
        Some(Commands::Undo { run_id }) => undo_run_command(run_id)?,
        None => println!("Use --help to see available commands."),
    }

    Ok(())
}

fn render_suggestions() -> anyhow::Result<()> {
    let conn = open_cli_database()?;
    let suggestions = list_suggestions(&conn).context("failed to read suggestions")?;

    if suggestions.is_empty() {
        println!("No suggestions stored.");
        return Ok(());
    }

    for suggestion in suggestions {
        println!("{}", suggestion.proposal_text);
        println!(
            "  pattern: {} | repeats: {} | avg duration: {} ms | score: {:.3} | freshness: {}",
            suggestion.canonical_summary,
            suggestion.count,
            suggestion.avg_duration_ms,
            suggestion.usefulness_score,
            suggestion.freshness
        );
    }

    Ok(())
}

fn render_patterns() -> anyhow::Result<()> {
    let conn = open_cli_database()?;
    let patterns = list_patterns(&conn).context("failed to read patterns")?;

    if patterns.is_empty() {
        println!("No patterns stored.");
        return Ok(());
    }

    let rows: Vec<Vec<String>> = patterns
        .into_iter()
        .map(|pattern| {
            vec![
                render_pattern_name(&pattern.signature),
                pattern.count.to_string(),
                render_pattern_example(&pattern.canonical_summary),
            ]
        })
        .collect();
    print_table(&["pattern_id", "runs", "example"], &rows);
    Ok(())
}

fn render_suggestions_table() -> anyhow::Result<()> {
    let conn = open_cli_database()?;
    let suggestions = list_suggestions(&conn).context("failed to read suggestions")?;

    if suggestions.is_empty() {
        println!("No suggestions stored.");
        return Ok(());
    }

    let rows: Vec<Vec<String>> = suggestions
        .into_iter()
        .map(|suggestion| {
            vec![
                suggestion.suggestion_id.to_string(),
                render_pattern_name(&suggestion.signature),
                suggestion.count.to_string(),
                format!("{:.3}", suggestion.usefulness_score),
                suggestion.freshness,
                suggestion.proposal_text,
            ]
        })
        .collect();
    print_table(
        &[
            "suggestion_id",
            "pattern",
            "runs",
            "score",
            "freshness",
            "description",
        ],
        &rows,
    );
    Ok(())
}

fn render_sessions() -> anyhow::Result<()> {
    let conn = open_cli_database()?;
    let sessions = list_recent_sessions(&conn, 20).context("failed to read sessions")?;

    if sessions.is_empty() {
        println!("No sessions stored.");
        return Ok(());
    }

    let rows: Vec<Vec<String>> = sessions
        .into_iter()
        .map(|session| {
            vec![
                session.session_id.to_string(),
                session.event_count.to_string(),
                format_duration(session.duration_ms),
            ]
        })
        .collect();
    print_table(&["session_id", "events", "duration"], &rows);
    Ok(())
}

fn approve_automation_command(suggestion_id: i64) -> anyhow::Result<()> {
    let mut conn = open_cli_database()?;
    let automation_id =
        approve_suggestion(&mut conn, suggestion_id).context("failed to approve suggestion")?;

    println!("Approved suggestion {suggestion_id} as automation {automation_id}.");
    Ok(())
}

fn render_automations() -> anyhow::Result<()> {
    let conn = open_cli_database()?;
    let automations = list_automations(&conn).context("failed to read automations")?;

    if automations.is_empty() {
        println!("No automations stored.");
        return Ok(());
    }

    let rows: Vec<Vec<String>> = automations
        .into_iter()
        .map(|automation| {
            vec![
                automation.automation_id.to_string(),
                automation
                    .suggestion_id
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "-".to_string()),
                automation.status,
                automation.summary,
            ]
        })
        .collect();
    print_table(
        &["automation_id", "suggestion_id", "status", "summary"],
        &rows,
    );
    Ok(())
}

fn disable_automation_command(automation_id: i64) -> anyhow::Result<()> {
    let conn = open_cli_database()?;
    disable_automation(&conn, automation_id).context("failed to disable automation")?;
    println!("Disabled automation {automation_id}.");
    Ok(())
}

fn enable_automation_command(automation_id: i64) -> anyhow::Result<()> {
    let conn = open_cli_database()?;
    enable_automation(&conn, automation_id).context("failed to enable automation")?;
    println!("Enabled automation {automation_id}.");
    Ok(())
}

fn dry_run_automation_command(automation_id: i64) -> anyhow::Result<()> {
    let conn = open_cli_database()?;
    let outcome =
        dry_run_automation(&conn, automation_id).context("failed to dry-run automation")?;

    for line in &outcome.preview {
        println!("{line}");
    }

    Ok(())
}

fn run_automation_command(automation_id: i64) -> anyhow::Result<()> {
    let conn = open_cli_database()?;
    let report =
        execute_automation(&conn, automation_id).context("failed to execute automation")?;

    if report.operations.is_empty() {
        println!("No matching files.");
    } else {
        for operation in &report.operations {
            println!(
                "{}: {} -> {}",
                operation.action, operation.from, operation.to
            );
        }
    }

    Ok(())
}

fn render_runs() -> anyhow::Result<()> {
    let conn = open_cli_database()?;
    let runs = list_runs(&conn).context("failed to read automation runs")?;

    if runs.is_empty() {
        println!("No automation runs stored.");
        return Ok(());
    }

    let rows: Vec<Vec<String>> = runs
        .into_iter()
        .map(|run| {
            vec![
                run.run_id.to_string(),
                run.automation_id.to_string(),
                run.result,
                run.started_at,
                run.finished_at.unwrap_or_else(|| "-".to_string()),
            ]
        })
        .collect();
    print_table(
        &[
            "run_id",
            "automation_id",
            "result",
            "started_at",
            "finished_at",
        ],
        &rows,
    );
    Ok(())
}

/// `flow-cli undo <run_id>` undoes one specific completed automation run using
/// the execution metadata captured when that run finished. The command never
/// performs bulk undo and will abort if the selected run cannot be reversed
/// safely.
fn undo_run_command(run_id: i64) -> anyhow::Result<()> {
    let conn = open_cli_database()?;
    let outcome = undo_automation_run(&conn, run_id).context("failed to undo automation run")?;

    for operation in &outcome.report.operations {
        println!(
            "{}: {} -> {}",
            operation.action, operation.from, operation.to
        );
    }
    println!("Undid automation run {}.", outcome.source_run_id);
    Ok(())
}

fn open_cli_database() -> anyhow::Result<rusqlite::Connection> {
    let db_path = std::env::var("FLOWD_DB_PATH")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| Config::default().database_path);
    open_database(&db_path).with_context(|| format!("failed to open database at {db_path}"))
}

fn render_pattern_name(signature: &str) -> String {
    let mut groups = Vec::new();
    for group in signature
        .split("->")
        .map(|part| part.split(':').nth(1).unwrap_or("file").replace('-', "_"))
    {
        if groups.last() != Some(&group) {
            groups.push(group);
        }
    }
    groups.push("workflow".to_string());
    groups.join("_")
}

fn render_pattern_example(summary: &str) -> String {
    summary
        .split(" -> ")
        .map(render_action_label)
        .collect::<Vec<_>>()
        .join(" -> ")
}

fn format_duration(duration_ms: i64) -> String {
    if duration_ms % 1000 == 0 {
        format!("{}s", duration_ms / 1000)
    } else {
        format!("{duration_ms}ms")
    }
}

fn print_table(headers: &[&str], rows: &[Vec<String>]) {
    let mut widths: Vec<usize> = headers.iter().map(|header| header.len()).collect();
    for row in rows {
        for (index, value) in row.iter().enumerate() {
            widths[index] = widths[index].max(value.len());
        }
    }

    println!("{}", format_row(headers.iter().copied(), &widths));
    println!(
        "{}",
        widths
            .iter()
            .map(|width| "-".repeat(*width))
            .collect::<Vec<_>>()
            .join("-+-")
    );
    for row in rows {
        println!("{}", format_row(row.iter(), &widths));
    }
}

fn format_row<'a, I, T>(values: I, widths: &[usize]) -> String
where
    I: IntoIterator<Item = T>,
    T: Display,
{
    values
        .into_iter()
        .enumerate()
        .map(|(index, value)| format!("{value:<width$}", width = widths[index]))
        .collect::<Vec<_>>()
        .join(" | ")
}

fn render_action_label(action: &str) -> String {
    let normalized = action.strip_suffix("File").unwrap_or(action);
    let mut label = String::new();

    for (index, ch) in normalized.chars().enumerate() {
        if ch.is_uppercase() && index > 0 {
            label.push(' ');
        }
        label.push(ch.to_ascii_lowercase());
    }

    label
}
