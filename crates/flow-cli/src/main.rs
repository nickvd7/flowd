use anyhow::Context;
use chrono::{DateTime, Utc};
use clap::{Parser, Subcommand};
use flow_analysis::intelligence_boundary::{
    ExplainabilitySource, IntelligenceBoundary, IntelligenceClient, NoopIntelligenceClient,
    SuggestionDecisionAction, SuggestionDisplayResult, SuggestionExplainability,
};
use flow_core::config::{expand_home, Config, ConfigSource, LoadedConfig};
use flow_db::{
    open_database,
    repo::{
        get_automation, get_suggestion, increment_rejected, increment_shown, increment_snoozed,
        list_automations, list_patterns, list_recent_sessions, list_suggestions,
        set_suggestion_status, StoredSuggestion,
    },
};
use flow_dsl::{Action, AutomationSpec};
use flow_exec::{
    approve_suggestion, disable_automation, dry_run_automation, enable_automation,
    execute_automation, list_runs, preview_automation, preview_suggestion, undo_automation_run,
    AutomationPreview,
};
use serde_json::Value;
use std::{fmt::Display, path::PathBuf};

#[derive(Debug, Parser)]
#[command(name = "flowctl", version, about = "CLI for flowd")]
struct Cli {
    #[arg(long, global = true, value_name = "PATH")]
    config: Option<PathBuf>,
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Debug, Subcommand)]
enum Commands {
    Status,
    Config {
        #[command(subcommand)]
        command: Option<ConfigCommand>,
    },
    Patterns,
    Suggest {
        #[arg(long)]
        explain: bool,
    },
    Suggestions {
        #[command(subcommand)]
        command: Option<SuggestionsCommand>,
        #[arg(long)]
        explain: bool,
    },
    Sessions,
    Tail,
    Approve {
        suggestion_id: i64,
    },
    Reject {
        suggestion_id: i64,
    },
    Snooze {
        suggestion_id: i64,
    },
    Automations {
        #[command(subcommand)]
        command: Option<AutomationsCommand>,
    },
    Disable {
        automation_id: i64,
    },
    Enable {
        automation_id: i64,
    },
    Run {
        automation_id: i64,
    },
    DryRun {
        automation_id: i64,
    },
    Runs,
    Undo {
        run_id: i64,
    },
}

#[derive(Debug, Subcommand)]
enum SuggestionsCommand {
    Explain { suggestion_id: i64 },
}

#[derive(Debug, Subcommand)]
enum AutomationsCommand {
    Show { automation_id: i64 },
}

#[derive(Debug, Subcommand)]
enum ConfigCommand {
    Show,
    Validate,
    Path,
}

#[derive(Debug, Clone)]
struct RuntimeContext {
    loaded_config: LoadedConfig,
}

fn main() {
    if let Err(error) = run() {
        eprintln!("error: {error:#}");
        std::process::exit(1);
    }
}

fn run() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let context = RuntimeContext {
        loaded_config: load_runtime_config(cli.config.as_deref())?,
    };

    match cli.command {
        Some(Commands::Status) => println!("flowd status: template skeleton"),
        Some(Commands::Config { command }) => render_config_command(&context, command)?,
        Some(Commands::Patterns) => render_patterns(&context)?,
        Some(Commands::Suggest { explain }) => render_suggestions(&context, explain)?,
        Some(Commands::Suggestions { command, explain }) => match command {
            Some(SuggestionsCommand::Explain { suggestion_id }) => {
                explain_suggestion_command(&context, suggestion_id)?
            }
            None => render_suggestions_table(&context, explain)?,
        },
        Some(Commands::Sessions) => render_sessions(&context)?,
        Some(Commands::Tail) => println!("tail: not implemented"),
        Some(Commands::Approve { suggestion_id }) => {
            approve_automation_command(&context, suggestion_id)?
        }
        Some(Commands::Reject { suggestion_id }) => {
            reject_suggestion_command(&context, suggestion_id)?
        }
        Some(Commands::Snooze { suggestion_id }) => {
            snooze_suggestion_command(&context, suggestion_id)?
        }
        Some(Commands::Automations { command }) => match command {
            Some(AutomationsCommand::Show { automation_id }) => {
                show_automation_command(&context, automation_id)?
            }
            None => render_automations(&context)?,
        },
        Some(Commands::Disable { automation_id }) => {
            disable_automation_command(&context, automation_id)?
        }
        Some(Commands::Enable { automation_id }) => {
            enable_automation_command(&context, automation_id)?
        }
        Some(Commands::Run { automation_id }) => run_automation_command(&context, automation_id)?,
        Some(Commands::DryRun { automation_id }) => {
            dry_run_automation_command(&context, automation_id)?
        }
        Some(Commands::Runs) => render_runs(&context)?,
        Some(Commands::Undo { run_id }) => undo_run_command(&context, run_id)?,
        None => println!("Use --help to see available commands."),
    }

    Ok(())
}

fn render_config_command(
    context: &RuntimeContext,
    command: Option<ConfigCommand>,
) -> anyhow::Result<()> {
    match command.unwrap_or(ConfigCommand::Show) {
        ConfigCommand::Show => {
            println!(
                "source = \"{}\"",
                render_config_source(&context.loaded_config.source)
            );
            println!();
            print!("{}", context.loaded_config.config.to_pretty_toml()?);
        }
        ConfigCommand::Validate => {
            context.loaded_config.config.validate()?;
            match &context.loaded_config.source {
                ConfigSource::File(path) => {
                    println!("Config is valid: {}", path.display());
                }
                ConfigSource::Default => {
                    println!("Config is valid: built-in defaults");
                }
            }
        }
        ConfigCommand::Path => match &context.loaded_config.source {
            ConfigSource::File(path) => println!("{}", path.display()),
            ConfigSource::Default => println!("built-in defaults"),
        },
    }

    Ok(())
}

fn render_suggestions(context: &RuntimeContext, explain: bool) -> anyhow::Result<()> {
    let mut conn = open_cli_database(context)?;
    let suggestions = suggestion_display_results(&conn, context, &NoopIntelligenceClient)?;

    if suggestions.is_empty() {
        println!("No suggestions stored.");
        return Ok(());
    }

    mark_suggestions_displayed_from_results(&mut conn, &suggestions)?;

    for suggestion in suggestions {
        println!(
            "[{}] {}",
            suggestion.suggestion.suggestion_id, suggestion.suggestion.proposal_text
        );
        println!(
            "  pattern: {} | runs: {} | avg: {} | score: {:.3} | freshness: {} | last seen: {}",
            suggestion.suggestion.canonical_summary,
            suggestion.suggestion.count,
            format_duration(suggestion.suggestion.avg_duration_ms),
            suggestion.suggestion.usefulness_score,
            suggestion.suggestion.freshness,
            format_timestamp(&suggestion.suggestion.last_seen_at)
        );
        if explain {
            for line in render_explainability_lines(&suggestion.explainability) {
                println!("  {line}");
            }
        }
    }

    Ok(())
}

fn render_patterns(context: &RuntimeContext) -> anyhow::Result<()> {
    let conn = open_cli_database(context)?;
    let patterns = list_patterns(&conn).context("failed to read patterns")?;

    if patterns.is_empty() {
        println!("No patterns stored.");
        return Ok(());
    }

    let rows: Vec<Vec<String>> = patterns
        .into_iter()
        .map(|pattern| {
            vec![
                pattern.pattern_id.to_string(),
                format!("{:.3}", pattern.usefulness_score),
                render_pattern_name(&pattern.signature),
                pattern.count.to_string(),
                format_duration(pattern.avg_duration_ms),
                format_timestamp(&pattern.last_seen_at),
                render_pattern_example(&pattern.canonical_summary),
            ]
        })
        .collect();
    print_table(
        &[
            "id",
            "score",
            "pattern",
            "runs",
            "avg",
            "last_seen",
            "example",
        ],
        &rows,
    );
    Ok(())
}

fn render_suggestions_table(context: &RuntimeContext, explain: bool) -> anyhow::Result<()> {
    let mut conn = open_cli_database(context)?;
    let suggestions = suggestion_display_results(&conn, context, &NoopIntelligenceClient)?;

    if suggestions.is_empty() {
        println!("No suggestions stored.");
        return Ok(());
    }

    mark_suggestions_displayed_from_results(&mut conn, &suggestions)?;
    print_table(
        &suggestion_table_headers(explain),
        &suggestion_display_rows(suggestions, explain),
    );
    Ok(())
}

fn explain_suggestion_command(context: &RuntimeContext, suggestion_id: i64) -> anyhow::Result<()> {
    let conn = open_cli_database(context)?;
    let resolved =
        resolve_suggestion_explanation(&conn, context, &NoopIntelligenceClient, suggestion_id)?;
    let preview =
        preview_suggestion(&conn, suggestion_id).context("failed to preview suggestion impact")?;

    for line in render_suggestion_explanation_report(&resolved, &preview) {
        println!("{line}");
    }

    Ok(())
}

fn render_sessions(context: &RuntimeContext) -> anyhow::Result<()> {
    let conn = open_cli_database(context)?;
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
                format_timestamp(&session.start_ts),
                format_timestamp(&session.end_ts),
            ]
        })
        .collect();
    print_table(&["id", "events", "duration", "start", "end"], &rows);
    Ok(())
}

fn approve_automation_command(context: &RuntimeContext, suggestion_id: i64) -> anyhow::Result<()> {
    let mut conn = open_cli_database(context)?;
    let automation_id =
        approve_suggestion(&mut conn, suggestion_id).context("failed to approve suggestion")?;

    println!("Approved suggestion {suggestion_id} as automation {automation_id}.");
    Ok(())
}

fn reject_suggestion_command(context: &RuntimeContext, suggestion_id: i64) -> anyhow::Result<()> {
    update_suggestion_feedback_status(context, suggestion_id, "rejected", increment_rejected)?;
    println!("Rejected suggestion {suggestion_id}.");
    Ok(())
}

fn snooze_suggestion_command(context: &RuntimeContext, suggestion_id: i64) -> anyhow::Result<()> {
    update_suggestion_feedback_status(context, suggestion_id, "snoozed", increment_snoozed)?;
    println!("Snoozed suggestion {suggestion_id}.");
    Ok(())
}

fn render_automations(context: &RuntimeContext) -> anyhow::Result<()> {
    let conn = open_cli_database(context)?;
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
                automation.run_count.to_string(),
                automation
                    .last_run_result
                    .unwrap_or_else(|| "-".to_string()),
                automation
                    .last_run_finished_at
                    .as_deref()
                    .map(format_timestamp)
                    .unwrap_or_else(|| "-".to_string()),
                automation
                    .accepted_at
                    .as_deref()
                    .map(format_timestamp)
                    .unwrap_or_else(|| "-".to_string()),
                automation.summary,
            ]
        })
        .collect();
    print_table(
        &[
            "id",
            "suggestion",
            "status",
            "runs",
            "last_run",
            "last_at",
            "accepted",
            "summary",
        ],
        &rows,
    );
    Ok(())
}

fn show_automation_command(context: &RuntimeContext, automation_id: i64) -> anyhow::Result<()> {
    let conn = open_cli_database(context)?;
    let automation = get_automation(&conn, automation_id)
        .context("failed to read automation")?
        .ok_or_else(|| anyhow::anyhow!("automation {automation_id} not found"))?;
    let spec = flow_dsl::parse_spec(&automation.spec_yaml).context("failed to parse automation")?;
    let preview =
        preview_automation(&conn, automation_id).context("failed to preview automation impact")?;

    for line in render_automation_report(&automation, &spec, &preview) {
        println!("{line}");
    }

    Ok(())
}

fn disable_automation_command(context: &RuntimeContext, automation_id: i64) -> anyhow::Result<()> {
    let conn = open_cli_database(context)?;
    disable_automation(&conn, automation_id).context("failed to disable automation")?;
    println!("Disabled automation {automation_id}.");
    Ok(())
}

fn enable_automation_command(context: &RuntimeContext, automation_id: i64) -> anyhow::Result<()> {
    let conn = open_cli_database(context)?;
    enable_automation(&conn, automation_id).context("failed to enable automation")?;
    println!("Enabled automation {automation_id}.");
    Ok(())
}

fn dry_run_automation_command(context: &RuntimeContext, automation_id: i64) -> anyhow::Result<()> {
    let conn = open_cli_database(context)?;
    let outcome =
        dry_run_automation(&conn, automation_id).context("failed to dry-run automation")?;

    for line in &outcome.preview {
        println!("{line}");
    }

    Ok(())
}

fn run_automation_command(context: &RuntimeContext, automation_id: i64) -> anyhow::Result<()> {
    let conn = open_cli_database(context)?;
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

fn render_runs(context: &RuntimeContext) -> anyhow::Result<()> {
    let conn = open_cli_database(context)?;
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
                summarize_run_operations(run.undo_payload_json.as_deref()).to_string(),
                format_timestamp(&run.started_at),
                run.finished_at
                    .as_deref()
                    .map(format_timestamp)
                    .unwrap_or_else(|| "-".to_string()),
            ]
        })
        .collect();
    print_table(
        &["id", "automation", "result", "ops", "started", "finished"],
        &rows,
    );
    Ok(())
}

/// `flow-cli undo <run_id>` undoes one specific completed automation run using
/// the execution metadata captured when that run finished. The command never
/// performs bulk undo and will abort if the selected run cannot be reversed
/// safely.
fn undo_run_command(context: &RuntimeContext, run_id: i64) -> anyhow::Result<()> {
    let conn = open_cli_database(context)?;
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

fn open_cli_database(context: &RuntimeContext) -> anyhow::Result<rusqlite::Connection> {
    let db_path = std::env::var("FLOWD_DB_PATH")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| context.loaded_config.config.database_path.clone());
    let db_path = expand_home(&db_path);
    open_database(&db_path)
        .with_context(|| format!("failed to open database at {}", db_path.display()))
}

fn suggestion_display_results(
    conn: &rusqlite::Connection,
    context: &RuntimeContext,
    intelligence_client: &dyn IntelligenceClient,
) -> anyhow::Result<Vec<SuggestionDisplayResult>> {
    let suggestions = list_suggestions(conn)
        .context("failed to read suggestions")?
        .into_iter()
        .filter(|suggestion| {
            suggestion.usefulness_score
                >= context.loaded_config.config.suggestion_min_usefulness_score
        })
        .collect::<Vec<_>>();

    if should_bypass_intelligence_ranking(&context.loaded_config.config) {
        return Ok(suggestions
            .into_iter()
            .map(|suggestion| SuggestionDisplayResult {
                explainability: baseline_fallback_explainability(suggestion.usefulness_score),
                suggestion,
                action: SuggestionDecisionAction::Keep,
            })
            .collect());
    }

    Ok(IntelligenceBoundary::new(intelligence_client)
        .evaluate_stored_suggestions_for_display(&suggestions)?
        .into_iter()
        .filter(|result| result.action == SuggestionDecisionAction::Keep)
        .collect())
}

#[derive(Debug, Clone, PartialEq)]
struct ResolvedSuggestionExplanation {
    suggestion: StoredSuggestion,
    action: SuggestionDecisionAction,
    explainability: SuggestionExplainability,
}

fn resolve_suggestion_explanation(
    conn: &rusqlite::Connection,
    context: &RuntimeContext,
    intelligence_client: &dyn IntelligenceClient,
    suggestion_id: i64,
) -> anyhow::Result<ResolvedSuggestionExplanation> {
    let suggestions = list_suggestions(conn).context("failed to read suggestions")?;
    if let Some(suggestion) = suggestions
        .iter()
        .find(|suggestion| suggestion.suggestion_id == suggestion_id)
        .cloned()
    {
        let explainability = if should_bypass_intelligence_ranking(&context.loaded_config.config) {
            ResolvedSuggestionExplanation {
                action: SuggestionDecisionAction::Keep,
                explainability: baseline_fallback_explainability(suggestion.usefulness_score),
                suggestion,
            }
        } else {
            IntelligenceBoundary::new(intelligence_client)
                .evaluate_stored_suggestions_for_display(&suggestions)?
                .into_iter()
                .find(|result| result.suggestion.suggestion_id == suggestion_id)
                .map(|result| ResolvedSuggestionExplanation {
                    suggestion: result.suggestion,
                    action: result.action,
                    explainability: result.explainability,
                })
                .expect("evaluated suggestion must exist in display results")
        };

        return Ok(explainability);
    }

    if let Some(suggestion) =
        get_suggestion(conn, suggestion_id).context("failed to read suggestion details")?
    {
        anyhow::bail!(
            "suggestion {suggestion_id} is not available for explanation; current status is {}",
            suggestion.status
        );
    }

    anyhow::bail!("suggestion {suggestion_id} not found");
}

fn render_suggestion_explanation_report(
    resolved: &ResolvedSuggestionExplanation,
    preview: &AutomationPreview,
) -> Vec<String> {
    let suggestion = &resolved.suggestion;
    let explainability = &resolved.explainability;
    let mut lines = vec![
        format!("suggestion: {}", suggestion.suggestion_id),
        "status: pending".to_string(),
        format!("decision: {}", render_decision_action(resolved.action)),
        format!(
            "source: {}",
            render_explainability_source(explainability.source)
        ),
        format!("pattern: {}", suggestion.canonical_summary),
        format!("proposal: {}", suggestion.proposal_text),
        format!("score: {:.3}", suggestion.usefulness_score),
        format!("freshness: {}", suggestion.freshness),
        format!("last_seen: {}", format_timestamp(&suggestion.last_seen_at)),
        format!("summary: {}", explainability.summary),
        format!(
            "rank: {}",
            explainability
                .rank_hint
                .map(|value| (value + 1).to_string())
                .unwrap_or_else(|| "baseline".to_string())
        ),
    ];

    if !explainability.score_breakdown.is_empty() {
        lines.push("score_breakdown:".to_string());
        lines.extend(
            explainability
                .score_breakdown
                .iter()
                .map(|component| format!("  {}={:.3}", component.label, component.value)),
        );
    }

    if let Some(reason) = &explainability.timing_reason {
        lines.push(format!("timing_reason: {reason}"));
    }

    if let Some(reason) = &explainability.suppression_reason {
        lines.push(format!("suppression_reason: {reason}"));
    }

    if !explainability.ranking_factors.is_empty() {
        lines.push("ranking_factors:".to_string());
        lines.extend(
            explainability
                .ranking_factors
                .iter()
                .map(|factor| format!("  {}={}", factor.label, factor.detail)),
        );
    }

    lines.push(format!(
        "feedback: shown={}, accepted={}, rejected={}, snoozed={}",
        suggestion.shown_count,
        suggestion.accepted_count,
        suggestion.rejected_count,
        suggestion.snoozed_count
    ));

    if let Some(value) = &suggestion.last_shown_ts {
        lines.push(format!("last_shown: {}", format_timestamp(value)));
    }
    if let Some(value) = &suggestion.last_accepted_ts {
        lines.push(format!("last_accepted: {}", format_timestamp(value)));
    }
    if let Some(value) = &suggestion.last_rejected_ts {
        lines.push(format!("last_rejected: {}", format_timestamp(value)));
    }
    if let Some(value) = &suggestion.last_snoozed_ts {
        lines.push(format!("last_snoozed: {}", format_timestamp(value)));
    }

    lines.push(String::new());
    lines.extend(render_automation_preview(preview));

    lines
}

fn mark_suggestions_displayed_from_results(
    conn: &mut rusqlite::Connection,
    suggestions: &[SuggestionDisplayResult],
) -> anyhow::Result<()> {
    let plain: Vec<_> = suggestions
        .iter()
        .map(|result| result.suggestion.clone())
        .collect();
    mark_suggestions_displayed(conn, &plain)
}

fn render_explainability_summary(explainability: &SuggestionExplainability) -> String {
    let rank = explainability
        .rank_hint
        .map(|value| format!("rank {}", value + 1))
        .unwrap_or_else(|| "rank baseline".to_string());
    format!(
        "{} | {} | {}",
        render_explainability_source(explainability.source),
        rank,
        explainability.summary
    )
}

fn render_explainability_lines(explainability: &SuggestionExplainability) -> Vec<String> {
    let mut lines = vec![format!(
        "explain: {} | {}",
        render_explainability_source(explainability.source),
        explainability.summary
    )];
    if let Some(rank_hint) = explainability.rank_hint {
        lines.push(format!("rank: {}", rank_hint + 1));
    }
    if !explainability.score_breakdown.is_empty() {
        lines.push(format!(
            "score: {}",
            explainability
                .score_breakdown
                .iter()
                .map(|component| format!("{}={:.3}", component.label, component.value))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    if let Some(reason) = &explainability.timing_reason {
        lines.push(format!("timing: {reason}"));
    }
    if let Some(reason) = &explainability.suppression_reason {
        lines.push(format!("suppression: {reason}"));
    }
    if !explainability.ranking_factors.is_empty() {
        lines.push(format!(
            "factors: {}",
            explainability
                .ranking_factors
                .iter()
                .map(|factor| format!("{}={}", factor.label, factor.detail))
                .collect::<Vec<_>>()
                .join("; ")
        ));
    }
    lines
}

fn render_explainability_source(source: ExplainabilitySource) -> &'static str {
    match source {
        ExplainabilitySource::Intelligence => "intelligence",
        ExplainabilitySource::BaselineFallback => "baseline fallback",
        ExplainabilitySource::MissingMetadata => "missing metadata",
    }
}

fn render_decision_action(action: SuggestionDecisionAction) -> &'static str {
    match action {
        SuggestionDecisionAction::Keep => "shown",
        SuggestionDecisionAction::Delay => "delayed",
        SuggestionDecisionAction::Suppress => "suppressed",
    }
}

fn baseline_fallback_explainability(score: f64) -> SuggestionExplainability {
    SuggestionExplainability {
        source: ExplainabilitySource::BaselineFallback,
        action: SuggestionDecisionAction::Keep,
        rank_hint: None,
        summary:
            "Open-core baseline order and wording were used because intelligence was unavailable."
                .to_string(),
        score_breakdown: vec![
            flow_analysis::intelligence_boundary::IntelligenceScoreComponent {
                label: "baseline_score".to_string(),
                value: score,
            },
        ],
        timing_reason: None,
        suppression_reason: None,
        ranking_factors: vec![
            flow_analysis::intelligence_boundary::IntelligenceRankingFactor {
                label: "fallback".to_string(),
                detail: "No intelligence decision was applied.".to_string(),
            },
        ],
    }
}

fn suggestion_display_rows(
    results: Vec<SuggestionDisplayResult>,
    explain: bool,
) -> Vec<Vec<String>> {
    results
        .into_iter()
        .map(|suggestion| {
            let mut row = vec![
                suggestion.suggestion.suggestion_id.to_string(),
                format!("{:.3}", suggestion.suggestion.usefulness_score),
                render_pattern_name(&suggestion.suggestion.signature),
                suggestion.suggestion.count.to_string(),
                format_duration(suggestion.suggestion.avg_duration_ms),
                suggestion.suggestion.freshness,
                format_timestamp(&suggestion.suggestion.last_seen_at),
                suggestion.suggestion.proposal_text,
            ];
            if explain {
                row.push(render_explainability_summary(&suggestion.explainability));
            }
            row
        })
        .collect()
}

fn suggestion_table_headers(explain: bool) -> Vec<&'static str> {
    if explain {
        vec![
            "id",
            "score",
            "pattern",
            "runs",
            "avg",
            "freshness",
            "last_seen",
            "description",
            "explain",
        ]
    } else {
        vec![
            "id",
            "score",
            "pattern",
            "runs",
            "avg",
            "freshness",
            "last_seen",
            "description",
        ]
    }
}

fn mark_suggestions_displayed(
    conn: &mut rusqlite::Connection,
    suggestions: &[StoredSuggestion],
) -> anyhow::Result<()> {
    let tx = conn
        .transaction()
        .context("failed to start suggestion display transaction")?;

    for suggestion in suggestions {
        increment_shown(&tx, suggestion.suggestion_id).with_context(|| {
            format!(
                "failed to record display for suggestion {}",
                suggestion.suggestion_id
            )
        })?;
    }

    tx.commit()
        .context("failed to commit suggestion display transaction")?;
    Ok(())
}

fn update_suggestion_feedback_status(
    context: &RuntimeContext,
    suggestion_id: i64,
    status: &str,
    increment_feedback: fn(&rusqlite::Connection, i64) -> rusqlite::Result<usize>,
) -> anyhow::Result<()> {
    let mut conn = open_cli_database(context)?;
    let tx = conn
        .transaction()
        .context("failed to start suggestion feedback transaction")?;
    increment_feedback(&tx, suggestion_id).with_context(|| {
        format!("failed to record {status} feedback for suggestion {suggestion_id}")
    })?;
    let updated = set_suggestion_status(&tx, suggestion_id, status)
        .with_context(|| format!("failed to set suggestion {suggestion_id} status to {status}"))?;
    if updated == 0 {
        anyhow::bail!("suggestion {suggestion_id} not found");
    }
    tx.commit()
        .context("failed to commit suggestion feedback transaction")?;
    Ok(())
}

fn should_bypass_intelligence_ranking(config: &Config) -> bool {
    if !config.intelligence_enabled {
        return true;
    }

    std::env::var("FLOWD_BYPASS_INTELLIGENCE_RANKING")
        .ok()
        .map(|value| {
            let normalized = value.trim().to_ascii_lowercase();
            normalized == "1" || normalized == "true" || normalized == "yes"
        })
        .unwrap_or(false)
}

fn load_runtime_config(config_path: Option<&std::path::Path>) -> anyhow::Result<LoadedConfig> {
    Config::load(config_path).map_err(Into::into)
}

fn render_config_source(source: &ConfigSource) -> String {
    match source {
        ConfigSource::Default => "default".to_string(),
        ConfigSource::File(path) => path.display().to_string(),
    }
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
    let total_seconds = duration_ms / 1000;

    if duration_ms >= 60_000 && duration_ms % 1000 == 0 {
        let minutes = total_seconds / 60;
        let seconds = total_seconds % 60;
        if seconds == 0 {
            return format!("{minutes}m");
        }
        return format!("{minutes}m {seconds}s");
    }

    if duration_ms % 1000 == 0 {
        return format!("{total_seconds}s");
    }

    format!("{duration_ms}ms")
}

fn format_timestamp(value: &str) -> String {
    if value.trim().is_empty() {
        return "-".to_string();
    }

    DateTime::parse_from_rfc3339(value)
        .map(|timestamp| {
            timestamp
                .with_timezone(&Utc)
                .format("%Y-%m-%d %H:%M:%SZ")
                .to_string()
        })
        .unwrap_or_else(|_| value.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::Result;
    use flow_analysis::intelligence_boundary::{
        display_stored_suggestions, rank_stored_suggestions, ExplainabilitySource,
        IntelligenceDisplayDecision, IntelligenceRankingFactor, IntelligenceRequest,
        IntelligenceResponse, IntelligenceScoreComponent, SuggestionDecisionAction,
    };

    struct RankingClient;

    impl IntelligenceClient for RankingClient {
        fn evaluate(&self, request: &IntelligenceRequest) -> Result<IntelligenceResponse> {
            Ok(IntelligenceResponse {
                decisions: request
                    .candidates
                    .iter()
                    .map(|candidate| IntelligenceDisplayDecision {
                        pattern_signature: candidate.pattern_signature.clone(),
                        action: SuggestionDecisionAction::Keep,
                        proposal_text: None,
                        usefulness_score: None,
                        rank_hint: Some(if candidate.pattern_signature.ends_with('b') {
                            0
                        } else {
                            1
                        }),
                        explanation: None,
                    })
                    .collect(),
            })
        }
    }

    fn stored_suggestion(
        suggestion_id: i64,
        signature: &str,
        usefulness_score: f64,
    ) -> StoredSuggestion {
        StoredSuggestion {
            suggestion_id,
            pattern_id: suggestion_id,
            signature: signature.to_string(),
            count: 2,
            avg_duration_ms: 10_000,
            canonical_summary: "CreateFile -> RenameFile".to_string(),
            proposal_text: format!("Proposal for {signature}"),
            usefulness_score,
            freshness: "current".to_string(),
            last_seen_at: "2026-01-15T10:00:00+00:00".to_string(),
            created_at: "2026-01-15T10:00:00+00:00".to_string(),
            shown_count: 0,
            accepted_count: 0,
            rejected_count: 0,
            snoozed_count: 0,
            last_shown_ts: None,
            last_accepted_ts: None,
            last_rejected_ts: None,
            last_snoozed_ts: None,
        }
    }

    #[test]
    fn display_ranking_uses_intelligence_when_available() {
        let suggestions = vec![
            stored_suggestion(1, "CreateFile:invoice-a", 0.9),
            stored_suggestion(2, "CreateFile:invoice-b", 0.8),
        ];

        let ranked = rank_stored_suggestions(&suggestions, &RankingClient).unwrap();

        assert_eq!(ranked[0].signature, "CreateFile:invoice-b");
        assert_eq!(ranked[1].signature, "CreateFile:invoice-a");
    }

    #[test]
    fn display_ranking_falls_back_to_existing_order_without_intelligence() {
        let suggestions = vec![
            stored_suggestion(1, "CreateFile:invoice-a", 0.9),
            stored_suggestion(2, "CreateFile:invoice-b", 0.8),
        ];

        let ranked = rank_stored_suggestions(&suggestions, &NoopIntelligenceClient).unwrap();

        assert_eq!(ranked, suggestions);
    }

    #[test]
    fn display_ranking_is_deterministic() {
        let suggestions = vec![
            stored_suggestion(1, "CreateFile:invoice-a", 0.9),
            stored_suggestion(2, "CreateFile:invoice-b", 0.8),
        ];

        let first = rank_stored_suggestions(&suggestions, &RankingClient).unwrap();
        let second = rank_stored_suggestions(&suggestions, &RankingClient).unwrap();

        assert_eq!(first, second);
    }

    #[test]
    fn display_decisions_can_hide_and_reword_suggestions() {
        struct DisplayClient;

        impl IntelligenceClient for DisplayClient {
            fn evaluate(&self, request: &IntelligenceRequest) -> Result<IntelligenceResponse> {
                Ok(IntelligenceResponse {
                    decisions: request
                        .candidates
                        .iter()
                        .map(|candidate| IntelligenceDisplayDecision {
                            pattern_signature: candidate.pattern_signature.clone(),
                            action: if candidate.pattern_signature.ends_with('a') {
                                SuggestionDecisionAction::Keep
                            } else {
                                SuggestionDecisionAction::Delay
                            },
                            proposal_text: Some(format!(
                                "Display: {}",
                                candidate.suggestion.baseline_proposal_text
                            )),
                            usefulness_score: None,
                            rank_hint: Some(if candidate.pattern_signature.ends_with('b') {
                                0
                            } else {
                                1
                            }),
                            explanation: None,
                        })
                        .collect(),
                })
            }
        }

        let suggestions = vec![
            stored_suggestion(1, "CreateFile:invoice-a", 0.9),
            stored_suggestion(2, "CreateFile:invoice-b", 0.8),
        ];

        let displayed = display_stored_suggestions(&suggestions, &DisplayClient).unwrap();

        assert_eq!(displayed.len(), 1);
        assert_eq!(displayed[0].signature, "CreateFile:invoice-a");
        assert_eq!(
            displayed[0].proposal_text,
            "Display: Proposal for CreateFile:invoice-a"
        );
    }

    #[test]
    fn display_fallback_stays_deterministic_without_intelligence() {
        let suggestions = vec![
            stored_suggestion(1, "CreateFile:invoice-a", 0.9),
            stored_suggestion(2, "CreateFile:invoice-b", 0.8),
        ];

        let first = display_stored_suggestions(&suggestions, &NoopIntelligenceClient).unwrap();
        let second = display_stored_suggestions(&suggestions, &NoopIntelligenceClient).unwrap();

        assert_eq!(first, suggestions);
        assert_eq!(first, second);
    }

    #[test]
    fn bypass_flag_disables_intelligence_ranking() {
        // The command path should remain easy to bypass even when a client exists.
        let config = Config::default();
        unsafe {
            std::env::set_var("FLOWD_BYPASS_INTELLIGENCE_RANKING", "true");
        }
        assert!(should_bypass_intelligence_ranking(&config));
        unsafe {
            std::env::remove_var("FLOWD_BYPASS_INTELLIGENCE_RANKING");
        }
        assert!(!should_bypass_intelligence_ranking(&config));
    }

    #[test]
    fn config_can_disable_intelligence_without_env_flags() {
        let config = Config {
            intelligence_enabled: false,
            ..Config::default()
        };

        assert!(should_bypass_intelligence_ranking(&config));
    }

    #[test]
    fn explainability_summary_stays_compact_and_readable() {
        let summary = render_explainability_summary(&SuggestionExplainability {
            source: ExplainabilitySource::Intelligence,
            action: SuggestionDecisionAction::Keep,
            rank_hint: Some(0),
            summary: "Recent usage kept this suggestion first.".to_string(),
            score_breakdown: vec![IntelligenceScoreComponent {
                label: "baseline_score".to_string(),
                value: 0.91,
            }],
            timing_reason: None,
            suppression_reason: None,
            ranking_factors: vec![IntelligenceRankingFactor {
                label: "usage".to_string(),
                detail: "The workflow repeated this morning.".to_string(),
            }],
        });

        assert_eq!(
            summary,
            "intelligence | rank 1 | Recent usage kept this suggestion first."
        );
    }

    #[test]
    fn explainability_lines_include_timing_and_suppression_details() {
        let lines = render_explainability_lines(&SuggestionExplainability {
            source: ExplainabilitySource::Intelligence,
            action: SuggestionDecisionAction::Delay,
            rank_hint: Some(1),
            summary: "Display timing was adjusted.".to_string(),
            score_breakdown: vec![
                IntelligenceScoreComponent {
                    label: "baseline_score".to_string(),
                    value: 0.75,
                },
                IntelligenceScoreComponent {
                    label: "delay_penalty".to_string(),
                    value: -0.15,
                },
            ],
            timing_reason: Some("A newer suggestion should be shown first.".to_string()),
            suppression_reason: Some("A similar suggestion is already active.".to_string()),
            ranking_factors: vec![IntelligenceRankingFactor {
                label: "freshness".to_string(),
                detail: "Newer activity received priority.".to_string(),
            }],
        });

        assert_eq!(
            lines[0],
            "explain: intelligence | Display timing was adjusted."
        );
        assert!(lines.contains(&"rank: 2".to_string()));
        assert!(lines.contains(&"timing: A newer suggestion should be shown first.".to_string()));
        assert!(lines.contains(&"suppression: A similar suggestion is already active.".to_string()));
    }

    #[test]
    fn baseline_fallback_explainability_is_explicit_and_deterministic() {
        let first = baseline_fallback_explainability(0.9);
        let second = baseline_fallback_explainability(0.9);

        assert_eq!(first, second);
        assert_eq!(first.source, ExplainabilitySource::BaselineFallback);
        assert_eq!(
            first.summary,
            "Open-core baseline order and wording were used because intelligence was unavailable."
        );
    }

    #[test]
    fn suggestion_explanation_report_uses_intelligence_metadata_when_available() {
        let resolved = ResolvedSuggestionExplanation {
            suggestion: stored_suggestion(7, "CreateFile:invoice-a", 0.9),
            action: SuggestionDecisionAction::Delay,
            explainability: SuggestionExplainability {
                source: ExplainabilitySource::Intelligence,
                action: SuggestionDecisionAction::Delay,
                rank_hint: Some(1),
                summary: "Display timing was adjusted.".to_string(),
                score_breakdown: vec![
                    IntelligenceScoreComponent {
                        label: "baseline_score".to_string(),
                        value: 0.9,
                    },
                    IntelligenceScoreComponent {
                        label: "recency_penalty".to_string(),
                        value: -0.2,
                    },
                ],
                timing_reason: Some("A newer suggestion should be shown first.".to_string()),
                suppression_reason: None,
                ranking_factors: vec![IntelligenceRankingFactor {
                    label: "recency".to_string(),
                    detail: "A more recent workflow was prioritized.".to_string(),
                }],
            },
        };

        let lines = render_suggestion_explanation_report(&resolved, &sample_preview());

        assert!(lines.contains(&"decision: delayed".to_string()));
        assert!(lines.contains(&"source: intelligence".to_string()));
        assert!(lines.contains(&"rank: 2".to_string()));
        assert!(
            lines.contains(&"timing_reason: A newer suggestion should be shown first.".to_string())
        );
        assert!(lines.contains(&"  recency_penalty=-0.200".to_string()));
        assert!(lines.contains(&"  recency=A more recent workflow was prioritized.".to_string()));
    }

    #[test]
    fn suggestion_explanation_report_preserves_suppression_reason() {
        let resolved = ResolvedSuggestionExplanation {
            suggestion: stored_suggestion(8, "CreateFile:invoice-b", 0.8),
            action: SuggestionDecisionAction::Suppress,
            explainability: SuggestionExplainability {
                source: ExplainabilitySource::Intelligence,
                action: SuggestionDecisionAction::Suppress,
                rank_hint: Some(2),
                summary: "A similar suggestion is already active.".to_string(),
                score_breakdown: vec![IntelligenceScoreComponent {
                    label: "baseline_score".to_string(),
                    value: 0.8,
                }],
                timing_reason: None,
                suppression_reason: Some("A similar suggestion is already active.".to_string()),
                ranking_factors: vec![IntelligenceRankingFactor {
                    label: "clustering".to_string(),
                    detail: "Similar suggestions were clustered.".to_string(),
                }],
            },
        };

        let lines = render_suggestion_explanation_report(&resolved, &sample_preview());

        assert!(lines.contains(&"decision: suppressed".to_string()));
        assert!(lines
            .contains(&"suppression_reason: A similar suggestion is already active.".to_string()));
        assert!(lines.contains(&"  clustering=Similar suggestions were clustered.".to_string()));
    }

    #[test]
    fn automation_preview_rendering_is_stable_and_compact() {
        let lines = render_automation_preview(&sample_preview());

        assert_eq!(
            lines,
            vec![
                "Automation preview".to_string(),
                String::new(),
                "Estimated impact:".to_string(),
                "- affects 2 files".to_string(),
                String::new(),
                "Examples:".to_string(),
                "- invoice-1001.pdf -> invoice-1001-reviewed.pdf".to_string(),
                "- invoice-1002.pdf -> invoice-1002-reviewed.pdf".to_string(),
                String::new(),
                "Destination:".to_string(),
                "- /tmp/archive".to_string(),
                String::new(),
                "Risk:".to_string(),
                "- low".to_string(),
                String::new(),
                "Action summary:".to_string(),
                "- rename".to_string(),
                "- move".to_string(),
            ]
        );
    }

    fn sample_preview() -> AutomationPreview {
        AutomationPreview {
            estimated_affected_files: Some(2),
            exact_count: true,
            examples: vec![
                flow_exec::PreviewExample {
                    before: "invoice-1001.pdf".to_string(),
                    after: "invoice-1001-reviewed.pdf".to_string(),
                },
                flow_exec::PreviewExample {
                    before: "invoice-1002.pdf".to_string(),
                    after: "invoice-1002-reviewed.pdf".to_string(),
                },
            ],
            destination_paths: vec!["/tmp/archive".to_string()],
            action_summary: vec!["rename".to_string(), "move".to_string()],
            risk: flow_exec::PreviewRisk::Low,
            notes: Vec::new(),
        }
    }
}

fn summarize_run_operations(payload: Option<&str>) -> usize {
    let Some(payload) = payload else {
        return 0;
    };

    let Ok(value) = serde_json::from_str::<Value>(payload) else {
        return 0;
    };

    value
        .get("operations")
        .and_then(|operations| operations.as_array())
        .map(|operations| operations.len())
        .or_else(|| {
            value
                .get("report")
                .and_then(|report| report.get("operations"))
                .and_then(|operations| operations.as_array())
                .map(|operations| operations.len())
        })
        .unwrap_or(0)
}

fn render_automation_report(
    automation: &flow_db::repo::StoredAutomationSpec,
    spec: &AutomationSpec,
    preview: &AutomationPreview,
) -> Vec<String> {
    let mut lines = vec![
        format!("automation: {}", automation.automation_id),
        format!("spec_id: {}", spec.id),
        format!("title: {}", render_optional_value(&automation.summary)),
        format!("status: {}", automation.status),
        format!(
            "suggestion: {}",
            automation
                .suggestion_id
                .map(|value| value.to_string())
                .unwrap_or_else(|| "-".to_string())
        ),
        format!(
            "accepted: {}",
            automation
                .accepted_at
                .as_deref()
                .map(format_timestamp)
                .unwrap_or_else(|| "-".to_string())
        ),
        "trigger:".to_string(),
        format!("  type: {}", spec.trigger.r#type),
        format!(
            "  path: {}",
            render_optional_value(spec.trigger.path.as_deref().unwrap_or(""))
        ),
        format!(
            "  extension: {}",
            render_optional_value(spec.trigger.extension.as_deref().unwrap_or(""))
        ),
        format!(
            "  name_contains: {}",
            render_optional_value(spec.trigger.name_contains.as_deref().unwrap_or(""))
        ),
        "actions:".to_string(),
    ];

    if spec.actions.is_empty() {
        lines.push("  - none".to_string());
    } else {
        lines.extend(
            spec.actions.iter().enumerate().map(|(index, action)| {
                format!("  {}. {}", index + 1, render_action_details(action))
            }),
        );
    }

    lines.push("safety:".to_string());
    if let Some(safety) = &spec.safety {
        lines.push(format!("  dry_run_first: {}", safety.dry_run_first));
        lines.push(format!("  undo_log: {}", safety.undo_log));
    } else {
        lines.push("  - none".to_string());
    }

    lines.push(String::new());
    lines.extend(render_automation_preview(preview));

    lines
}

fn render_automation_preview(preview: &AutomationPreview) -> Vec<String> {
    let mut lines = vec![
        "Automation preview".to_string(),
        String::new(),
        "Estimated impact:".to_string(),
    ];

    let impact_line = match preview.estimated_affected_files {
        Some(count) if preview.exact_count => format!("- affects {count} files"),
        Some(count) => format!("- affects approximately {count} files"),
        None => "- affected file count unavailable".to_string(),
    };
    lines.push(impact_line);

    lines.push(String::new());
    lines.push("Examples:".to_string());
    if preview.examples.is_empty() {
        lines.push("- no representative examples available".to_string());
    } else {
        lines.extend(
            preview
                .examples
                .iter()
                .map(|example| format!("- {} -> {}", example.before, example.after)),
        );
    }

    lines.push(String::new());
    lines.push("Destination:".to_string());
    if preview.destination_paths.is_empty() {
        lines.push("- destination unavailable".to_string());
    } else {
        lines.extend(
            preview
                .destination_paths
                .iter()
                .map(|destination| format!("- {destination}")),
        );
    }

    lines.push(String::new());
    lines.push("Risk:".to_string());
    lines.push(format!("- {}", preview.risk.as_str()));

    lines.push(String::new());
    lines.push("Action summary:".to_string());
    if preview.action_summary.is_empty() {
        lines.push("- action summary unavailable".to_string());
    } else {
        lines.extend(
            preview
                .action_summary
                .iter()
                .map(|action| format!("- {action}")),
        );
    }

    if !preview.notes.is_empty() {
        lines.push(String::new());
        lines.push("Notes:".to_string());
        lines.extend(preview.notes.iter().map(|note| format!("- {note}")));
    }

    lines
}

fn render_action_details(action: &Action) -> String {
    match action {
        Action::Rename { template } => format!("rename template={template}"),
        Action::Move { destination } => format!("move destination={destination}"),
    }
}

fn render_optional_value(value: &str) -> String {
    if value.trim().is_empty() {
        "-".to_string()
    } else {
        value.to_string()
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
