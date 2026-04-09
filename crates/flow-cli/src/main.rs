use anyhow::Context;
use chrono::{DateTime, Utc};
use clap::{Parser, Subcommand, ValueEnum};
use flow_analysis::intelligence_boundary::{
    build_envelope_from_contexts, map_stored_suggestions_to_contexts, ExplainabilitySource,
    IntelligenceBoundary, IntelligenceClient, InternalFeedbackSummary, InternalPatternMetadata,
    InternalRecencySignals, InternalSessionSummary, InternalSuggestionHistory,
    NoopIntelligenceClient, SuggestionDecisionAction, SuggestionDisplayResult,
    SuggestionExplainability,
};
use flow_core::config::{
    expand_home, preferred_setup_config_path, Config, ConfigSource, LoadedConfig,
};
use flow_core::events::{ActionType, NormalizedEvent};
use flow_db::{
    open_database,
    repo::{
        get_automation, get_suggestion, increment_rejected, increment_shown, increment_snoozed,
        list_all_suggestion_records, list_automations, list_patterns, list_raw_events_after,
        list_recent_sessions, list_sessions, list_suggestions, list_suggestions_for_export,
        load_example_events_for_pattern, load_local_usage_stats, set_suggestion_status,
        LocalUsageStats, StoredPattern, StoredRawEvent, StoredSession, StoredSuggestion,
        StoredSuggestionForExport, StoredSuggestionRecord,
    },
};
use flow_dsl::{Action, AutomationSpec, WorkflowPackManifest};
use flow_exec::{
    approve_suggestion, disable_automation, dry_run_automation, enable_automation,
    execute_automation, list_runs, preview_automation, preview_suggestion, undo_automation_run,
    AutomationPreview,
};
use serde::Serialize;
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt::Display,
    fs,
    path::{Path, PathBuf},
    process::Command as ProcessCommand,
    thread,
    time::Duration,
};

const ROOT_AFTER_HELP: &str = "\
Examples:
  flowctl setup --watch ~/Downloads
  flowctl config show
  flowctl suggestions
  flowctl suggestions explain 1
  flowctl suggestions history
  flowctl approve 1
  flowctl automations show 1
  flowctl watch
  flowctl watch --events --patterns";

const CONFIG_AFTER_HELP: &str = "\
Examples:
  flowctl config show
  flowctl config validate
  flowctl config path";

const SUGGESTIONS_AFTER_HELP: &str = "\
Examples:
  flowctl suggestions
  flowctl suggestions --explain
  flowctl suggestions explain 1
  flowctl suggestions show 1
  flowctl suggestions history
  flowctl approve 1";

const AUTOMATIONS_AFTER_HELP: &str = "\
Examples:
  flowctl automations
  flowctl automations show 1
  flowctl dry-run 1
  flowctl run 1
  flowctl runs";

const INTELLIGENCE_AFTER_HELP: &str = "\
Examples:
  flowctl intelligence export-feedback --output ./feedback-export.json
  flowctl intelligence export-feedback --output ./feedback-export.json --generated-at 2026-03-13T12:00:00+00:00";

#[derive(Debug, Parser)]
#[command(
    name = "flowctl",
    version,
    about = "Inspect local workflow suggestions, automations, and config for flowd",
    long_about = None,
    arg_required_else_help = true,
    after_help = ROOT_AFTER_HELP
)]
struct Cli {
    #[arg(long, global = true, value_name = "PATH")]
    config: Option<PathBuf>,
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Debug, Subcommand)]
enum Commands {
    #[command(about = "Show flowd status")]
    Status,
    #[command(about = "Run lightweight local health checks for flowd")]
    Doctor,
    #[command(about = "Create or update a local flowd config")]
    Setup {
        #[arg(long = "watch", value_name = "PATH")]
        watch: Vec<String>,
        #[arg(long)]
        force: bool,
    },
    #[command(
        about = "Inspect the resolved flowd config",
        after_help = CONFIG_AFTER_HELP
    )]
    Config {
        #[command(subcommand)]
        command: Option<ConfigCommand>,
    },
    #[command(about = "Show local workflow and automation totals")]
    Stats,
    #[command(about = "List detected repeated workflow patterns")]
    Patterns,
    #[command(about = "Print concise suggestion summaries")]
    Suggest {
        #[arg(long)]
        explain: bool,
    },
    #[command(
        about = "List suggestions and inspect explainability or feedback history",
        after_help = SUGGESTIONS_AFTER_HELP
    )]
    Suggestions {
        #[command(subcommand)]
        command: Option<SuggestionsCommand>,
        #[arg(long)]
        explain: bool,
    },
    #[command(about = "List recent workflow sessions")]
    Sessions,
    #[command(about = "Manage local workflow packs")]
    Packs {
        #[command(subcommand)]
        command: Option<PacksCommand>,
    },
    #[command(
        about = "Export local intelligence evaluation facts and feedback",
        after_help = INTELLIGENCE_AFTER_HELP
    )]
    Intelligence {
        #[command(subcommand)]
        command: IntelligenceCommand,
    },
    #[command(
        about = "Watch flowd activity as it is observed and inferred",
        visible_alias = "tail"
    )]
    Watch {
        #[arg(long, help = "Show observed events")]
        events: bool,
        #[arg(long, help = "Show pattern changes")]
        patterns: bool,
        #[arg(long, help = "Show suggestion changes")]
        suggestions: bool,
        #[arg(
            long = "category",
            value_name = "NAME",
            value_enum,
            num_args = 1..,
            value_delimiter = ','
        )]
        categories: Vec<WatchCategory>,
        #[arg(long, default_value_t = 1000, value_name = "MS")]
        poll_interval_ms: u64,
        #[arg(long)]
        once: bool,
    },
    #[command(about = "Approve a suggestion into a deterministic automation")]
    Approve { suggestion_id: i64 },
    #[command(about = "Reject a suggestion and hide it from pending results")]
    Reject { suggestion_id: i64 },
    #[command(about = "Snooze a suggestion and hide it from pending results")]
    Snooze { suggestion_id: i64 },
    #[command(
        about = "List approved automations and inspect one in detail",
        after_help = AUTOMATIONS_AFTER_HELP
    )]
    Automations {
        #[command(subcommand)]
        command: Option<AutomationsCommand>,
    },
    #[command(about = "Disable an automation without deleting it")]
    Disable { automation_id: i64 },
    #[command(about = "Re-enable a disabled automation")]
    Enable { automation_id: i64 },
    #[command(about = "Execute an automation against matching files")]
    Run { automation_id: i64 },
    #[command(about = "Preview an automation without changing files")]
    DryRun { automation_id: i64 },
    #[command(about = "List automation run history")]
    Runs,
    #[command(about = "Undo one completed automation run")]
    Undo { run_id: i64 },
}

#[derive(Debug, Subcommand)]
enum SuggestionsCommand {
    #[command(about = "Show explainability and preview details for one suggestion")]
    Explain { suggestion_id: i64 },
    #[command(about = "Show suggestion feedback history")]
    History,
    #[command(about = "Show detailed feedback fields for one suggestion")]
    Show { suggestion_id: i64 },
}

#[derive(Debug, Subcommand)]
enum AutomationsCommand {
    #[command(about = "Show one automation with preview details")]
    Show { automation_id: i64 },
}

#[derive(Debug, Subcommand)]
enum PacksCommand {
    #[command(about = "List installed workflow packs")]
    List,
    #[command(about = "Validate a workflow pack directory")]
    Validate { path: PathBuf },
}

#[derive(Debug, Subcommand)]
enum IntelligenceCommand {
    #[command(about = "Export suggestion feedback and evaluation context to a local JSON file")]
    ExportFeedback {
        #[arg(long, value_name = "PATH")]
        output: PathBuf,
        #[arg(long, value_name = "RFC3339_TIMESTAMP", hide = true)]
        generated_at: Option<String>,
    },
}

#[derive(Debug, Subcommand)]
enum ConfigCommand {
    #[command(about = "Print the resolved config values")]
    Show,
    #[command(about = "Validate the resolved config")]
    Validate,
    #[command(about = "Print the config source path")]
    Path,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, ValueEnum)]
enum WatchCategory {
    Events,
    Sessions,
    Patterns,
    Suggestions,
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

    if let Some(Commands::Setup { watch, force }) = &cli.command {
        return setup_command(cli.config.as_deref(), watch, *force);
    }

    let context = RuntimeContext {
        loaded_config: load_runtime_config(cli.config.as_deref())?,
    };

    match cli.command {
        Some(Commands::Status) => println!("flowd status: template skeleton"),
        Some(Commands::Doctor) => render_doctor(&context)?,
        Some(Commands::Setup { .. }) => unreachable!("setup is handled before runtime config"),
        Some(Commands::Config { command }) => render_config_command(&context, command)?,
        Some(Commands::Stats) => render_stats(&context)?,
        Some(Commands::Patterns) => render_patterns(&context)?,
        Some(Commands::Suggest { explain }) => render_suggestions(&context, explain)?,
        Some(Commands::Suggestions { command, explain }) => match command {
            Some(SuggestionsCommand::Explain { suggestion_id }) => {
                explain_suggestion_command(&context, suggestion_id)?
            }
            Some(SuggestionsCommand::History) => render_suggestion_history(&context)?,
            Some(SuggestionsCommand::Show { suggestion_id }) => {
                show_suggestion_history_command(&context, suggestion_id)?
            }
            None => render_suggestions_table(&context, explain)?,
        },
        Some(Commands::Sessions) => render_sessions(&context)?,
        Some(Commands::Packs { command }) => match command.unwrap_or(PacksCommand::List) {
            PacksCommand::List => render_packs_list(&context)?,
            PacksCommand::Validate { path } => validate_pack_command(&path)?,
        },
        Some(Commands::Intelligence { command }) => match command {
            IntelligenceCommand::ExportFeedback {
                output,
                generated_at,
            } => export_feedback_command(&context, &output, generated_at.as_deref())?,
        },
        Some(Commands::Watch {
            events,
            patterns,
            suggestions,
            categories,
            poll_interval_ms,
            once,
        }) => render_watch(
            &context,
            &watch_category_filters(events, patterns, suggestions, &categories),
            poll_interval_ms,
            once,
        )?,
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
        None => unreachable!("clap handles missing commands"),
    }

    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SetupAction {
    Created,
    Updated,
    Unchanged,
}

fn setup_command(
    explicit_config_path: Option<&Path>,
    watch: &[String],
    force: bool,
) -> anyhow::Result<()> {
    let target_path = explicit_config_path
        .map(Path::to_path_buf)
        .map(Ok)
        .unwrap_or_else(preferred_setup_config_path)
        .map_err(anyhow::Error::from)?;
    let existing_config = if target_path.is_file() {
        Some(Config::load_from_path(&target_path).with_context(|| {
            format!(
                "failed to load existing config at {}",
                target_path.display()
            )
        })?)
    } else {
        None
    };

    let desired_config = build_setup_config(existing_config.clone(), watch);
    let action = if existing_config.is_none() {
        write_setup_config(&target_path, &desired_config)?;
        SetupAction::Created
    } else if force {
        write_setup_config(&target_path, &desired_config)?;
        SetupAction::Updated
    } else {
        SetupAction::Unchanged
    };

    let reported_config = if action == SetupAction::Unchanged {
        existing_config
            .as_ref()
            .expect("existing config must be available when setup is unchanged")
    } else {
        &desired_config
    };

    for line in render_setup_report(
        action,
        &target_path,
        reported_config,
        !watch.is_empty() && existing_config.is_some() && !force,
    ) {
        println!("{line}");
    }

    Ok(())
}

fn build_setup_config(existing_config: Option<Config>, watch: &[String]) -> Config {
    let mut config = existing_config.unwrap_or_default();
    if !watch.is_empty() {
        config.observed_folders = watch.to_vec();
    }
    config
}

fn write_setup_config(path: &Path, config: &Config) -> anyhow::Result<()> {
    config.validate()?;

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create config directory {}", parent.display()))?;
    }

    fs::write(path, config.to_pretty_toml()?)
        .with_context(|| format!("failed to write config to {}", path.display()))?;
    Ok(())
}

fn render_setup_report(
    action: SetupAction,
    config_path: &Path,
    config: &Config,
    skipped_watch_update: bool,
) -> Vec<String> {
    let flowctl_prefix = format!("flowctl --config {}", shell_quote(config_path));
    let daemon_command = format!("flow-daemon --config {}", shell_quote(config_path));
    let mut lines = vec![
        match action {
            SetupAction::Created => format!("Created config: {}", config_path.display()),
            SetupAction::Updated => format!("Updated config: {}", config_path.display()),
            SetupAction::Unchanged => format!("Config already exists: {}", config_path.display()),
        },
        format!("Observed folders: {}", config.observed_folders.join(", ")),
    ];

    if action == SetupAction::Unchanged {
        lines.push("No changes were made.".to_string());
    }

    if skipped_watch_update {
        lines.push(
            "Requested watched paths were not applied. Re-run with --force to rewrite the config."
                .to_string(),
        );
    }

    lines.push(String::new());
    lines.extend(render_next_steps(&[
        format!("Start the daemon: {daemon_command}"),
        format!("Inspect generated config: {flowctl_prefix} config show"),
        format!("Inspect suggestions: {flowctl_prefix} suggestions"),
        format!("Review local stats: {flowctl_prefix} stats"),
    ]));

    lines
}

fn render_next_steps(steps: &[String]) -> Vec<String> {
    let mut lines = vec!["Next steps:".to_string()];
    lines.extend(
        steps
            .iter()
            .enumerate()
            .map(|(index, step)| format!("{}. {step}", index + 1)),
    );
    lines
}

fn render_packs_list(_context: &RuntimeContext) -> anyhow::Result<()> {
    // v0.1: placeholder until we add a persistent pack registry.
    println!("No workflow packs are installed yet.");
    println!("Use 'flowctl packs validate <path>' to validate a local pack folder.");
    Ok(())
}

fn validate_pack_command(pack_dir: &Path) -> anyhow::Result<()> {
    let manifest_path = pack_dir.join("workflow-pack.toml");
    let manifest_str = fs::read_to_string(&manifest_path).with_context(|| {
        format!("failed to read workflow pack manifest at {}", manifest_path.display())
    })?;

    let manifest: WorkflowPackManifest = flow_dsl::parse_pack_manifest(&manifest_str)
        .with_context(|| format!("failed to parse manifest at {}", manifest_path.display()))?;

    println!("Pack id: {}", manifest.pack.id);
    println!("Name: {}", manifest.pack.name);
    println!("Version: {}", manifest.pack.version);
    if let Some(description) = manifest.pack.description.as_deref() {
        println!("Description: {description}");
    }
    println!("Automations: {}", manifest.automation.len());
    println!();

    let mut had_error = false;

    for automation_ref in &manifest.automation {
        let spec_path = pack_dir.join(&automation_ref.file);
        println!("Validating automation spec: {}", spec_path.display());
        let yaml = match fs::read_to_string(&spec_path) {
            Ok(contents) => contents,
            Err(error) => {
                eprintln!("  error: failed to read spec: {error}");
                had_error = true;
                continue;
            }
        };

        match flow_dsl::parse_spec(&yaml) {
            Ok(spec) => {
                println!("  ok: id='{}', actions={}", spec.id, spec.actions.len());
            }
            Err(error) => {
                eprintln!("  error: failed to parse automation spec: {error}");
                had_error = true;
            }
        }
    }

    if had_error {
        anyhow::bail!("one or more automation specs failed validation");
    }

    Ok(())
}

fn shell_quote(path: &Path) -> String {
    let rendered = path.display().to_string();
    if rendered
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '/' | '-' | '_' | '.' | '~'))
    {
        return rendered;
    }

    format!(
        "\"{}\"",
        rendered.replace('\\', "\\\\").replace('"', "\\\"")
    )
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

    let first_suggestion_id = suggestions
        .first()
        .map(|result| result.suggestion.suggestion_id);
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

    if let Some(suggestion_id) = first_suggestion_id {
        println!();
        for line in render_next_steps(&[
            format!("Inspect one suggestion: flowctl suggestions explain {suggestion_id}"),
            "Review suggestion history: flowctl suggestions history".to_string(),
            format!("Approve a suggestion: flowctl approve {suggestion_id}"),
        ]) {
            println!("{line}");
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

fn render_stats(context: &RuntimeContext) -> anyhow::Result<()> {
    let conn = open_cli_database(context)?;
    let stats = load_local_usage_stats(&conn).context("failed to read local usage stats")?;

    for line in render_stats_report(&stats) {
        println!("{line}");
    }

    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DoctorReport {
    daemon: String,
    database: String,
    watch_paths: String,
    events_observed: String,
    patterns_detected: String,
    suggestions_available: String,
    automations: String,
    intelligence_layer: String,
}

fn render_doctor(context: &RuntimeContext) -> anyhow::Result<()> {
    for line in render_doctor_report(&doctor_report(context)) {
        println!("{line}");
    }

    Ok(())
}

fn doctor_report(context: &RuntimeContext) -> DoctorReport {
    let config = &context.loaded_config.config;
    let daemon = if is_doctor_daemon_running() {
        "running".to_string()
    } else {
        "not running".to_string()
    };

    let watch_paths = if configured_watch_paths(config).is_empty() {
        "not configured".to_string()
    } else {
        "configured".to_string()
    };

    let intelligence_layer = if config.intelligence_enabled {
        match NoopIntelligenceClient.evaluate(&default_intelligence_request()) {
            Ok(_) => "connected".to_string(),
            Err(error) => format!("unavailable ({error})"),
        }
    } else {
        "disabled".to_string()
    };

    match open_doctor_database(context) {
        Ok(conn) => {
            let events_observed = doctor_has_rows(&conn, "raw_events")
                .map(render_yes_no)
                .unwrap_or_else(|error| format!("unknown ({error})"));
            let patterns_detected = doctor_has_active_patterns(&conn)
                .map(render_yes_no)
                .unwrap_or_else(|error| format!("unknown ({error})"));
            let suggestions_available = list_suggestions(&conn)
                .map(|suggestions| render_yes_no(!suggestions.is_empty()))
                .unwrap_or_else(|error| format!("unknown ({error})"));
            let automations = list_automations(&conn)
                .map(|automations| {
                    let active = automations
                        .iter()
                        .filter(|automation| automation.status == "active")
                        .count();
                    if active == 0 {
                        "none active".to_string()
                    } else if active == 1 {
                        "1 active".to_string()
                    } else {
                        format!("{active} active")
                    }
                })
                .unwrap_or_else(|error| format!("unknown ({error})"));

            DoctorReport {
                daemon,
                database: "ok".to_string(),
                watch_paths,
                events_observed,
                patterns_detected,
                suggestions_available,
                automations,
                intelligence_layer,
            }
        }
        Err(error) => {
            let message = error.to_string();
            DoctorReport {
                daemon,
                database: format!("error ({message})"),
                watch_paths,
                events_observed: "unknown".to_string(),
                patterns_detected: "unknown".to_string(),
                suggestions_available: "unknown".to_string(),
                automations: "unknown".to_string(),
                intelligence_layer,
            }
        }
    }
}

fn render_doctor_report(report: &DoctorReport) -> Vec<String> {
    vec![
        format!("daemon: {}", report.daemon),
        format!("database: {}", report.database),
        format!("watch paths: {}", report.watch_paths),
        format!("events observed: {}", report.events_observed),
        format!("patterns detected: {}", report.patterns_detected),
        format!("suggestions available: {}", report.suggestions_available),
        format!("automations: {}", report.automations),
        format!("intelligence layer: {}", report.intelligence_layer),
    ]
}

fn configured_watch_paths(config: &Config) -> Vec<PathBuf> {
    config
        .observed_folders
        .iter()
        .map(|folder| expand_home(folder))
        .filter(|path| path.is_dir())
        .collect()
}

fn open_doctor_database(context: &RuntimeContext) -> anyhow::Result<rusqlite::Connection> {
    let db_path = std::env::var("FLOWD_DB_PATH")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| context.loaded_config.config.database_path.clone());
    let db_path = expand_home(&db_path);
    rusqlite::Connection::open_with_flags(&db_path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
        .with_context(|| format!("failed to open database at {}", db_path.display()))
}

fn doctor_has_rows(conn: &rusqlite::Connection, table: &str) -> anyhow::Result<bool> {
    let sql = format!("SELECT EXISTS(SELECT 1 FROM {table} LIMIT 1)");
    let exists = conn.query_row(&sql, [], |row| row.get::<_, i64>(0))?;
    Ok(exists != 0)
}

fn doctor_has_active_patterns(conn: &rusqlite::Connection) -> anyhow::Result<bool> {
    let exists = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM patterns WHERE is_active = 1 LIMIT 1)",
        [],
        |row| row.get::<_, i64>(0),
    )?;
    Ok(exists != 0)
}

fn render_yes_no(value: bool) -> String {
    if value {
        "yes".to_string()
    } else {
        "no".to_string()
    }
}

fn default_intelligence_request() -> flow_analysis::intelligence_boundary::IntelligenceRequest {
    flow_analysis::intelligence_boundary::IntelligenceRequest {
        context: Default::default(),
        candidates: Vec::new(),
    }
}

fn is_doctor_daemon_running() -> bool {
    if let Some(value) = std::env::var("FLOWD_DOCTOR_DAEMON_RUNNING")
        .ok()
        .map(|value| value.trim().to_ascii_lowercase())
    {
        return matches!(value.as_str(), "1" | "true" | "yes");
    }

    let output = match ProcessCommand::new("ps").args(["-axo", "comm="]).output() {
        Ok(output) => output,
        Err(_) => return false,
    };

    if !output.status.success() {
        return false;
    }

    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .any(|line| line == "flow-daemon" || line.ends_with("/flow-daemon"))
}

fn render_suggestions_table(context: &RuntimeContext, explain: bool) -> anyhow::Result<()> {
    let mut conn = open_cli_database(context)?;
    let suggestions = suggestion_display_results(&conn, context, &NoopIntelligenceClient)?;

    if suggestions.is_empty() {
        println!("No suggestions stored.");
        return Ok(());
    }

    let first_suggestion_id = suggestions
        .first()
        .map(|result| result.suggestion.suggestion_id);
    mark_suggestions_displayed_from_results(&mut conn, &suggestions)?;
    print_table(
        &suggestion_table_headers(explain),
        &suggestion_display_rows(suggestions, explain),
    );
    if let Some(suggestion_id) = first_suggestion_id {
        println!();
        for line in render_next_steps(&[
            format!("Inspect one suggestion: flowctl suggestions explain {suggestion_id}"),
            "Review suggestion history: flowctl suggestions history".to_string(),
            format!("Approve a suggestion: flowctl approve {suggestion_id}"),
        ]) {
            println!("{line}");
        }
    }
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

fn render_suggestion_history(context: &RuntimeContext) -> anyhow::Result<()> {
    let conn = open_cli_database(context)?;
    let suggestions =
        list_all_suggestion_records(&conn).context("failed to read suggestion history")?;

    if suggestions.is_empty() {
        println!("No suggestion history stored.");
        return Ok(());
    }

    let rows: Vec<Vec<String>> = suggestions
        .into_iter()
        .map(|suggestion| {
            let latest = render_latest_interaction(&suggestion);
            vec![
                suggestion.suggestion_id.to_string(),
                suggestion.status,
                render_pattern_name(&suggestion.signature),
                suggestion.shown_count.to_string(),
                suggestion.accepted_count.to_string(),
                suggestion.rejected_count.to_string(),
                suggestion.snoozed_count.to_string(),
                latest,
                suggestion.proposal_text,
            ]
        })
        .collect();
    print_table(
        &[
            "id",
            "status",
            "pattern",
            "shown",
            "accepted",
            "rejected",
            "snoozed",
            "latest",
            "description",
        ],
        &rows,
    );
    Ok(())
}

fn show_suggestion_history_command(
    context: &RuntimeContext,
    suggestion_id: i64,
) -> anyhow::Result<()> {
    let conn = open_cli_database(context)?;
    let suggestion = list_all_suggestion_records(&conn)
        .context("failed to read suggestion history")?
        .into_iter()
        .find(|suggestion| suggestion.suggestion_id == suggestion_id)
        .ok_or_else(|| anyhow::anyhow!("suggestion {suggestion_id} not found"))?;

    for line in render_suggestion_history_report(&suggestion) {
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

const INTELLIGENCE_FEEDBACK_EXPORT_SCHEMA: &str = "flowd.intelligence_feedback_export";
const INTELLIGENCE_FEEDBACK_EXPORT_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Serialize)]
struct IntelligenceFeedbackExport {
    schema_name: &'static str,
    export_version: u32,
    generated_at: String,
    context: IntelligenceFeedbackExportContext,
    suggestion_records: Vec<IntelligenceFeedbackSuggestionRecord>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
struct IntelligenceFeedbackExportContext {
    reference_ts: Option<String>,
    candidate_count: usize,
    session_summary: InternalSessionSummary,
    feedback_summary: InternalFeedbackSummary,
    local_usage_stats: IntelligenceFeedbackLocalUsageStats,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct IntelligenceFeedbackLocalUsageStats {
    pattern_count: usize,
    suggestion_count: usize,
    approved_automation_count: usize,
    automation_run_count: usize,
    undo_run_count: usize,
    estimated_time_saved_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
struct IntelligenceFeedbackSuggestionRecord {
    suggestion_id: i64,
    pattern_signature: String,
    status: String,
    suggestion: IntelligenceFeedbackSuggestionMetadata,
    feedback: InternalSuggestionHistory,
    evaluation_context: IntelligenceFeedbackEvaluationContext,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
struct IntelligenceFeedbackSuggestionMetadata {
    canonical_summary: String,
    proposal_text: String,
    usefulness_score: f64,
    count: usize,
    avg_duration_ms: i64,
    freshness: String,
    last_seen_at: String,
    created_at: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
struct IntelligenceFeedbackEvaluationContext {
    pattern: InternalPatternMetadata,
    recency: InternalRecencySignals,
}

fn export_feedback_command(
    context: &RuntimeContext,
    output_path: &Path,
    generated_at: Option<&str>,
) -> anyhow::Result<()> {
    let conn = open_cli_database(context)?;
    let suggestions =
        list_suggestions_for_export(&conn).context("failed to read suggestions for export")?;
    let sessions = list_sessions(&conn).context("failed to read sessions for export")?;
    let usage_stats =
        load_local_usage_stats(&conn).context("failed to read local usage stats for export")?;
    let generated_at = generated_at
        .map(|value| {
            DateTime::parse_from_rfc3339(value)
                .map(|timestamp| timestamp.to_rfc3339())
                .with_context(|| format!("failed to parse generated timestamp {value} as RFC3339"))
        })
        .transpose()?
        .unwrap_or_else(|| Utc::now().to_rfc3339());
    let export =
        build_intelligence_feedback_export(suggestions, &sessions, &usage_stats, generated_at);
    let json = serde_json::to_string_pretty(&export)
        .context("failed to serialize intelligence feedback export")?;

    if let Some(parent) = output_path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent).with_context(|| {
                format!("failed to create export directory {}", parent.display())
            })?;
        }
    }
    fs::write(output_path, format!("{json}\n"))
        .with_context(|| format!("failed to write export to {}", output_path.display()))?;

    println!(
        "Exported {} suggestion records to {}",
        export.suggestion_records.len(),
        output_path.display()
    );
    Ok(())
}

fn build_intelligence_feedback_export(
    suggestions: Vec<StoredSuggestionForExport>,
    sessions: &[StoredSession],
    usage_stats: &LocalUsageStats,
    generated_at: String,
) -> IntelligenceFeedbackExport {
    let stored_suggestions: Vec<_> = suggestions
        .iter()
        .map(StoredSuggestionForExport::as_stored_suggestion)
        .collect();
    let contexts = map_stored_suggestions_to_contexts(&stored_suggestions);
    let envelope =
        build_envelope_from_contexts(&contexts, Some(summarize_stored_sessions(sessions)));

    let suggestion_records = suggestions
        .into_iter()
        .zip(envelope.candidates)
        .map(
            |(suggestion, context)| IntelligenceFeedbackSuggestionRecord {
                suggestion_id: suggestion.suggestion_id,
                pattern_signature: suggestion.signature,
                status: suggestion.status,
                suggestion: IntelligenceFeedbackSuggestionMetadata {
                    canonical_summary: suggestion.canonical_summary,
                    proposal_text: suggestion.proposal_text,
                    usefulness_score: suggestion.usefulness_score,
                    count: suggestion.count,
                    avg_duration_ms: suggestion.avg_duration_ms,
                    freshness: suggestion.freshness,
                    last_seen_at: suggestion.last_seen_at,
                    created_at: suggestion.created_at,
                },
                feedback: context.history,
                evaluation_context: IntelligenceFeedbackEvaluationContext {
                    pattern: context.pattern,
                    recency: context.recency,
                },
            },
        )
        .collect();

    IntelligenceFeedbackExport {
        schema_name: INTELLIGENCE_FEEDBACK_EXPORT_SCHEMA,
        export_version: INTELLIGENCE_FEEDBACK_EXPORT_VERSION,
        generated_at,
        context: IntelligenceFeedbackExportContext {
            reference_ts: envelope.context.reference_ts,
            candidate_count: envelope.context.candidate_count,
            session_summary: envelope.context.session_summary,
            feedback_summary: envelope.context.feedback_summary,
            local_usage_stats: IntelligenceFeedbackLocalUsageStats {
                pattern_count: usage_stats.pattern_count,
                suggestion_count: usage_stats.suggestion_count,
                approved_automation_count: usage_stats.approved_automation_count,
                automation_run_count: usage_stats.automation_run_count,
                undo_run_count: usage_stats.undo_run_count,
                estimated_time_saved_ms: usage_stats.estimated_time_saved_ms,
            },
        },
        suggestion_records,
    }
}

fn summarize_stored_sessions(sessions: &[StoredSession]) -> InternalSessionSummary {
    if sessions.is_empty() {
        return InternalSessionSummary::default();
    }

    let total_duration_ms: i64 = sessions.iter().map(|session| session.duration_ms).sum();
    let total_events: usize = sessions.iter().map(|session| session.event_count).sum();
    let latest_session_end_ts = sessions.iter().map(|session| session.end_ts.clone()).max();

    InternalSessionSummary {
        total_sessions: sessions.len(),
        avg_session_duration_ms: total_duration_ms / sessions.len() as i64,
        avg_events_per_session: total_events / sessions.len(),
        latest_session_end_ts,
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct WatchSessionFingerprint {
    start_ts: String,
    end_ts: String,
    event_count: usize,
    duration_ms: i64,
}

#[derive(Debug, Clone, PartialEq)]
struct WatchPatternSnapshot {
    pattern_id: i64,
    signature: String,
    count: usize,
    avg_duration_ms: i64,
    canonical_summary: String,
    usefulness_score: f64,
    last_seen_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct WatchSuggestionSnapshot {
    suggestion_id: i64,
    pattern_id: i64,
    status: String,
    signature: String,
    canonical_summary: String,
    proposal_text: String,
}

#[derive(Debug, Clone, Default)]
struct WatchState {
    last_raw_event_id: i64,
    seen_sessions: BTreeSet<WatchSessionFingerprint>,
    patterns: BTreeMap<i64, WatchPatternSnapshot>,
    suggestions: BTreeMap<i64, WatchSuggestionSnapshot>,
}

fn render_watch(
    context: &RuntimeContext,
    categories: &[WatchCategory],
    poll_interval_ms: u64,
    once: bool,
) -> anyhow::Result<()> {
    let categories = selected_watch_categories(categories);
    let poll_interval = Duration::from_millis(poll_interval_ms.max(100));
    let mut state = if once {
        WatchState::default()
    } else {
        bootstrap_watch_state(context)?
    };

    if !once {
        println!(
            "Watching categories: {} (poll: {}ms)",
            render_watch_category_list(&categories),
            poll_interval.as_millis()
        );
    }

    loop {
        let conn = open_watch_database(context)?;
        let lines = collect_watch_lines(&conn, &categories, &mut state)?;
        for line in lines {
            println!("{line}");
        }

        if once {
            break;
        }

        thread::sleep(poll_interval);
    }

    Ok(())
}

fn bootstrap_watch_state(context: &RuntimeContext) -> anyhow::Result<WatchState> {
    let conn = open_watch_database(context)?;
    let raw_events = list_raw_events_after(&conn, 0).context("failed to read raw events")?;
    let sessions = list_sessions(&conn).context("failed to read sessions")?;
    let patterns = list_patterns(&conn).context("failed to read patterns")?;
    let suggestions =
        list_all_suggestion_records(&conn).context("failed to read suggestion history")?;

    Ok(WatchState {
        last_raw_event_id: raw_events.last().map(|event| event.id).unwrap_or(0),
        seen_sessions: sessions
            .into_iter()
            .map(|session| watch_session_fingerprint(&session))
            .collect(),
        patterns: patterns
            .into_iter()
            .map(|pattern| (pattern.pattern_id, watch_pattern_snapshot(&pattern)))
            .collect(),
        suggestions: suggestions
            .into_iter()
            .map(|suggestion| {
                (
                    suggestion.suggestion_id,
                    watch_suggestion_snapshot(&suggestion),
                )
            })
            .collect(),
    })
}

fn collect_watch_lines(
    conn: &rusqlite::Connection,
    categories: &BTreeSet<WatchCategory>,
    state: &mut WatchState,
) -> anyhow::Result<Vec<String>> {
    let mut lines = Vec::new();

    if categories.contains(&WatchCategory::Events) {
        let raw_events = list_raw_events_after(conn, state.last_raw_event_id)
            .context("failed to read raw events")?;
        if let Some(last) = raw_events.last() {
            state.last_raw_event_id = last.id;
        }
        lines.extend(raw_events.iter().filter_map(render_watch_raw_event));
    }

    if categories.contains(&WatchCategory::Sessions) {
        let sessions = list_sessions(conn).context("failed to read sessions")?;
        for session in sessions {
            let fingerprint = watch_session_fingerprint(&session);
            if state.seen_sessions.insert(fingerprint) {
                lines.push(render_watch_session(&session));
            }
        }
    }

    if categories.contains(&WatchCategory::Patterns) {
        let patterns = list_patterns(conn).context("failed to read patterns")?;
        for pattern in patterns {
            let snapshot = watch_pattern_snapshot(&pattern);
            match state.patterns.get(&pattern.pattern_id) {
                None => {
                    lines.push(render_watch_pattern_detected(&pattern));
                }
                Some(previous) if previous != &snapshot => {
                    lines.push(render_watch_pattern_updated(previous, &pattern));
                }
                Some(_) => {}
            }
            state.patterns.insert(pattern.pattern_id, snapshot);
        }
    }

    if categories.contains(&WatchCategory::Suggestions) {
        let suggestions =
            list_all_suggestion_records(conn).context("failed to read suggestion history")?;
        for suggestion in suggestions {
            let snapshot = watch_suggestion_snapshot(&suggestion);
            match state.suggestions.get(&suggestion.suggestion_id) {
                None => lines.push(render_watch_suggestion_created(&suggestion)),
                Some(previous) if previous != &snapshot => {
                    lines.push(render_watch_suggestion_updated(previous, &suggestion));
                }
                Some(_) => {}
            }
            state.suggestions.insert(suggestion.suggestion_id, snapshot);
        }
    }

    Ok(lines)
}

fn selected_watch_categories(categories: &[WatchCategory]) -> BTreeSet<WatchCategory> {
    if categories.is_empty() {
        return [
            WatchCategory::Events,
            WatchCategory::Patterns,
            WatchCategory::Suggestions,
        ]
        .into_iter()
        .collect();
    }

    categories.iter().copied().collect()
}

fn watch_category_filters(
    events: bool,
    patterns: bool,
    suggestions: bool,
    categories: &[WatchCategory],
) -> Vec<WatchCategory> {
    let mut filters = Vec::new();
    if events {
        filters.push(WatchCategory::Events);
    }
    if patterns {
        filters.push(WatchCategory::Patterns);
    }
    if suggestions {
        filters.push(WatchCategory::Suggestions);
    }
    filters.extend_from_slice(categories);
    filters
}

fn render_watch_category_list(categories: &BTreeSet<WatchCategory>) -> String {
    categories
        .iter()
        .map(|category| match category {
            WatchCategory::Events => "events",
            WatchCategory::Sessions => "sessions",
            WatchCategory::Patterns => "patterns",
            WatchCategory::Suggestions => "suggestions",
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn watch_session_fingerprint(session: &StoredSession) -> WatchSessionFingerprint {
    WatchSessionFingerprint {
        start_ts: session.start_ts.clone(),
        end_ts: session.end_ts.clone(),
        event_count: session.event_count,
        duration_ms: session.duration_ms,
    }
}

fn watch_pattern_snapshot(pattern: &StoredPattern) -> WatchPatternSnapshot {
    WatchPatternSnapshot {
        pattern_id: pattern.pattern_id,
        signature: pattern.signature.clone(),
        count: pattern.count,
        avg_duration_ms: pattern.avg_duration_ms,
        canonical_summary: pattern.canonical_summary.clone(),
        usefulness_score: pattern.usefulness_score,
        last_seen_at: pattern.last_seen_at.clone(),
    }
}

fn watch_suggestion_snapshot(suggestion: &StoredSuggestionRecord) -> WatchSuggestionSnapshot {
    WatchSuggestionSnapshot {
        suggestion_id: suggestion.suggestion_id,
        pattern_id: suggestion.pattern_id,
        status: suggestion.status.clone(),
        signature: suggestion.signature.clone(),
        canonical_summary: suggestion.canonical_summary.clone(),
        proposal_text: suggestion.proposal_text.clone(),
    }
}

fn render_watch_session(session: &StoredSession) -> String {
    format!(
        "[session] updated: {} events over {} ({} -> {})",
        session.event_count,
        format_duration(session.duration_ms),
        format_timestamp(&session.start_ts),
        format_timestamp(&session.end_ts),
    )
}

fn render_watch_pattern_detected(pattern: &StoredPattern) -> String {
    format!(
        "[pattern] candidate detected: {} (repetitions: {})",
        render_pattern_name(&pattern.signature),
        pattern.count,
    )
}

fn render_watch_pattern_updated(
    previous: &WatchPatternSnapshot,
    pattern: &StoredPattern,
) -> String {
    let action = if pattern.count > previous.count {
        "candidate strengthened"
    } else {
        "candidate updated"
    };
    format!(
        "[pattern] {action}: {} (repetitions: {})",
        render_pattern_name(&pattern.signature),
        pattern.count,
    )
}

fn render_watch_suggestion_created(suggestion: &StoredSuggestionRecord) -> String {
    format!("[suggestion] new: {}", suggestion.proposal_text)
}

fn render_watch_suggestion_updated(
    previous: &WatchSuggestionSnapshot,
    suggestion: &StoredSuggestionRecord,
) -> String {
    if previous.status != suggestion.status {
        return format!(
            "[suggestion] status changed: #{} {} -> {}",
            suggestion.suggestion_id, previous.status, suggestion.status
        );
    }

    format!(
        "[suggestion] updated: #{} {}",
        suggestion.suggestion_id, suggestion.proposal_text
    )
}

fn approve_automation_command(context: &RuntimeContext, suggestion_id: i64) -> anyhow::Result<()> {
    let mut conn = open_cli_database(context)?;
    let automation_id =
        approve_suggestion(&mut conn, suggestion_id).context("failed to approve suggestion")?;

    println!("Approved suggestion {suggestion_id} as automation {automation_id}.");
    println!();
    for line in render_next_steps(&[
        format!("Inspect the automation: flowctl automations show {automation_id}"),
        format!("Preview the automation: flowctl dry-run {automation_id}"),
        format!("Run the automation: flowctl run {automation_id}"),
    ]) {
        println!("{line}");
    }
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
    println!();
    for line in render_next_steps(&[
        format!("Preview this automation: flowctl dry-run {automation_id}"),
        format!("Run this automation: flowctl run {automation_id}"),
        "Review automation run history: flowctl runs".to_string(),
    ]) {
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

fn open_watch_database(context: &RuntimeContext) -> anyhow::Result<rusqlite::Connection> {
    let db_path = std::env::var("FLOWD_DB_PATH")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| context.loaded_config.config.database_path.clone());
    let db_path = expand_home(&db_path);
    rusqlite::Connection::open_with_flags(&db_path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
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
    example_events: Vec<NormalizedEvent>,
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
        let example_events = load_example_events_for_pattern(conn, suggestion.pattern_id)
            .context("failed to load stored workflow evidence")?;
        let explainability = if should_bypass_intelligence_ranking(&context.loaded_config.config) {
            ResolvedSuggestionExplanation {
                action: SuggestionDecisionAction::Keep,
                explainability: baseline_fallback_explainability(suggestion.usefulness_score),
                suggestion,
                example_events,
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
                    example_events: example_events.clone(),
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
    let mut lines = vec![
        format!("Suggestion: {}", suggestion.proposal_text),
        String::new(),
        "Why this suggestion appeared:".to_string(),
        String::new(),
        format!("pattern repetitions: {}", suggestion.count),
        format!("last seen: {}", format_timestamp(&suggestion.last_seen_at)),
        format!(
            "confidence: {} ({:.3})",
            render_confidence_label(suggestion.usefulness_score),
            suggestion.usefulness_score
        ),
        format!(
            "estimated time saved: ~{}",
            render_estimated_time_saved(suggestion.avg_duration_ms)
        ),
    ];

    let workflow_lines = render_observed_workflow(&resolved.example_events);
    lines.push(String::new());
    lines.push("Observed workflow:".to_string());
    if workflow_lines.is_empty() {
        lines.push("- no stored workflow steps available".to_string());
    } else {
        lines.extend(workflow_lines.into_iter().map(|line| format!("- {line}")));
    }

    lines.push(String::new());
    lines.push("Stored metadata:".to_string());
    lines.push(format!("pattern: {}", suggestion.canonical_summary));
    lines.push("status: pending".to_string());
    lines.push(format!("freshness: {}", suggestion.freshness));
    lines.push(format!(
        "feedback: shown={}, accepted={}, rejected={}, snoozed={}",
        suggestion.shown_count,
        suggestion.accepted_count,
        suggestion.rejected_count,
        suggestion.snoozed_count
    ));

    if resolved.action != SuggestionDecisionAction::Keep {
        lines.push(format!(
            "decision: {}",
            render_decision_action(resolved.action)
        ));
    }

    if resolved.explainability.source != ExplainabilitySource::BaselineFallback {
        lines.push(format!(
            "display source: {}",
            render_explainability_source(resolved.explainability.source)
        ));
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

fn render_confidence_label(score: f64) -> &'static str {
    match score {
        score if score >= 0.8 => "high",
        score if score >= 0.6 => "medium",
        _ => "low",
    }
}

fn render_estimated_time_saved(avg_duration_ms: i64) -> String {
    if avg_duration_ms >= 60_000 {
        let minutes = ((avg_duration_ms as f64) / 60_000.0).round() as i64;
        return format!("{minutes} minute{}", if minutes == 1 { "" } else { "s" });
    }

    format_duration(avg_duration_ms)
}

fn render_observed_workflow(events: &[NormalizedEvent]) -> Vec<String> {
    events
        .iter()
        .filter_map(render_observed_workflow_step)
        .collect()
}

fn render_observed_workflow_step(event: &NormalizedEvent) -> Option<String> {
    let terminal_command = event
        .metadata
        .get("source")
        .and_then(|value| value.as_str())
        .filter(|source| *source == "terminal")
        .and_then(|_| event.metadata.get("redacted_command"))
        .and_then(|value| value.as_str())
        .filter(|value| !value.trim().is_empty());
    if let Some(command) = terminal_command {
        return Some(command.to_string());
    }

    let target = event.target.as_deref()?;
    let from_path = event
        .metadata
        .get("from_path")
        .and_then(|value| value.as_str());

    match event.action_type {
        ActionType::CreateFile | ActionType::DownloadFile => {
            Some(format!("create {}", render_path_for_workflow(target)))
        }
        ActionType::RenameFile => Some(format!(
            "rename {} -> {}",
            from_path
                .map(render_path_for_workflow)
                .unwrap_or_else(|| "file".to_string()),
            render_path_for_workflow(target)
        )),
        ActionType::MoveFile => Some(format!(
            "move {} -> {}",
            from_path
                .map(render_path_for_workflow)
                .unwrap_or_else(|| "file".to_string()),
            render_path_for_workflow(target)
        )),
        _ => None,
    }
}

fn render_path_for_workflow(path: &str) -> String {
    let display = compact_home_path(path);
    let path = Path::new(&display);
    let file_name = path.file_name().and_then(|value| value.to_str());

    match file_name {
        Some(name) if matches!(path.parent().and_then(|value| value.to_str()), Some(".")) => {
            name.to_string()
        }
        Some(name) => {
            if let Some(parent) = path.parent().and_then(|value| value.to_str()) {
                if parent.is_empty() {
                    return name.to_string();
                }
                return format!("{name} ({parent})");
            }
            name.to_string()
        }
        None => display,
    }
}

fn compact_home_path(path: &str) -> String {
    let Some(home) = std::env::var_os("HOME") else {
        return path.to_string();
    };
    let home = PathBuf::from(home);
    let path_buf = Path::new(path);

    if path_buf == home {
        return "~".to_string();
    }

    match path_buf.strip_prefix(&home) {
        Ok(relative) => {
            let suffix = relative.display().to_string();
            if suffix.is_empty() {
                "~".to_string()
            } else {
                format!("~/{}", suffix)
            }
        }
        Err(_) => path.to_string(),
    }
}

fn render_latest_interaction(suggestion: &StoredSuggestionRecord) -> String {
    [
        ("shown", suggestion.last_shown_ts.as_deref()),
        ("accepted", suggestion.last_accepted_ts.as_deref()),
        ("rejected", suggestion.last_rejected_ts.as_deref()),
        ("snoozed", suggestion.last_snoozed_ts.as_deref()),
    ]
    .into_iter()
    .filter_map(|(label, value)| value.map(|timestamp| (label, timestamp)))
    .max_by(|left, right| left.1.cmp(right.1))
    .map(|(label, timestamp)| format!("{label} {}", format_timestamp(timestamp)))
    .unwrap_or_else(|| "-".to_string())
}

fn render_suggestion_history_report(suggestion: &StoredSuggestionRecord) -> Vec<String> {
    let mut lines = vec![
        format!("suggestion: {}", suggestion.suggestion_id),
        format!("status: {}", suggestion.status),
        format!("pattern: {}", suggestion.canonical_summary),
        format!("signature: {}", suggestion.signature),
        format!("proposal: {}", suggestion.proposal_text),
        format!(
            "feedback: shown={}, accepted={}, rejected={}, snoozed={}",
            suggestion.shown_count,
            suggestion.accepted_count,
            suggestion.rejected_count,
            suggestion.snoozed_count
        ),
        format!("latest: {}", render_latest_interaction(suggestion)),
    ];

    lines.push(format!(
        "last_shown: {}",
        suggestion
            .last_shown_ts
            .as_deref()
            .map(format_timestamp)
            .unwrap_or_else(|| "-".to_string())
    ));
    lines.push(format!(
        "last_accepted: {}",
        suggestion
            .last_accepted_ts
            .as_deref()
            .map(format_timestamp)
            .unwrap_or_else(|| "-".to_string())
    ));
    lines.push(format!(
        "last_rejected: {}",
        suggestion
            .last_rejected_ts
            .as_deref()
            .map(format_timestamp)
            .unwrap_or_else(|| "-".to_string())
    ));
    lines.push(format!(
        "last_snoozed: {}",
        suggestion
            .last_snoozed_ts
            .as_deref()
            .map(format_timestamp)
            .unwrap_or_else(|| "-".to_string())
    ));

    lines
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

fn render_stats_report(stats: &LocalUsageStats) -> Vec<String> {
    let mut lines = vec![
        "Local usage stats".to_string(),
        format!("patterns_detected: {}", stats.pattern_count),
        format!("suggestions_created: {}", stats.suggestion_count),
        format!("automations_approved: {}", stats.approved_automation_count),
        format!("automation_runs: {}", stats.automation_run_count),
        format!("undo_runs: {}", stats.undo_run_count),
        format!(
            "estimated_time_saved: {}",
            format_duration(stats.estimated_time_saved_ms)
        ),
    ];
    lines.push(String::new());
    lines.extend(render_next_steps(&[
        "Inspect pending suggestions: flowctl suggestions".to_string(),
        "Inspect approved automations: flowctl automations".to_string(),
        "Inspect config values: flowctl config show".to_string(),
    ]));
    lines
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

    fn stored_suggestion_for_export(
        suggestion_id: i64,
        signature: &str,
        usefulness_score: f64,
    ) -> StoredSuggestionForExport {
        StoredSuggestionForExport {
            suggestion_id,
            status: "pending".to_string(),
            pattern_id: suggestion_id,
            signature: signature.to_string(),
            count: 2,
            avg_duration_ms: 10_000,
            canonical_summary: "CreateFile -> RenameFile".to_string(),
            proposal_text: format!("Proposal for {signature}"),
            usefulness_score,
            freshness: "current".to_string(),
            last_seen_at: "2026-01-15T10:00:00+00:00".to_string(),
            created_at: "2026-01-15T09:00:00+00:00".to_string(),
            shown_count: 3,
            accepted_count: 1,
            rejected_count: 1,
            snoozed_count: 0,
            last_shown_ts: Some("2026-01-15T10:30:00+00:00".to_string()),
            last_accepted_ts: Some("2026-01-15T10:31:00+00:00".to_string()),
            last_rejected_ts: Some("2026-01-15T10:32:00+00:00".to_string()),
            last_snoozed_ts: None,
        }
    }

    fn sample_observed_workflow_events() -> Vec<NormalizedEvent> {
        vec![
            NormalizedEvent {
                ts: Utc::now(),
                action_type: ActionType::MoveFile,
                app: Some("terminal".to_string()),
                target: Some("/tmp/workspace/archive/report.txt".to_string()),
                metadata: serde_json::json!({
                    "source": "terminal",
                    "redacted_command": "mv <path> <path>",
                    "from_path": "/tmp/workspace/report.txt"
                }),
            },
            NormalizedEvent {
                ts: Utc::now(),
                action_type: ActionType::RenameFile,
                app: None,
                target: Some("/tmp/workspace/final.txt".to_string()),
                metadata: serde_json::json!({
                    "from_path": "/tmp/workspace/draft.txt"
                }),
            },
        ]
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
    fn stats_report_format_is_deterministic() {
        let stats = LocalUsageStats {
            pattern_count: 3,
            suggestion_count: 4,
            approved_automation_count: 2,
            automation_run_count: 5,
            undo_run_count: 1,
            estimated_time_saved_ms: 90_000,
        };

        assert_eq!(
            render_stats_report(&stats),
            vec![
                "Local usage stats".to_string(),
                "patterns_detected: 3".to_string(),
                "suggestions_created: 4".to_string(),
                "automations_approved: 2".to_string(),
                "automation_runs: 5".to_string(),
                "undo_runs: 1".to_string(),
                "estimated_time_saved: 1m 30s".to_string(),
                String::new(),
                "Next steps:".to_string(),
                "1. Inspect pending suggestions: flowctl suggestions".to_string(),
                "2. Inspect approved automations: flowctl automations".to_string(),
                "3. Inspect config values: flowctl config show".to_string(),
            ]
        );
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
            example_events: sample_observed_workflow_events(),
        };

        let lines = render_suggestion_explanation_report(&resolved, &sample_preview());

        assert_eq!(lines[0], "Suggestion: Proposal for CreateFile:invoice-a");
        assert!(lines.contains(&"pattern repetitions: 2".to_string()));
        assert!(lines.contains(&"confidence: high (0.900)".to_string()));
        assert!(lines.contains(&"estimated time saved: ~10s".to_string()));
        assert!(lines.contains(&"- mv <path> <path>".to_string()));
        assert!(lines.contains(
            &"- rename draft.txt (/tmp/workspace) -> final.txt (/tmp/workspace)".to_string()
        ));
        assert!(lines.contains(&"decision: delayed".to_string()));
        assert!(lines.contains(&"display source: intelligence".to_string()));
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
            example_events: Vec::new(),
        };

        let lines = render_suggestion_explanation_report(&resolved, &sample_preview());

        assert!(lines.contains(&"decision: suppressed".to_string()));
        assert!(lines.contains(&"display source: intelligence".to_string()));
        assert!(lines.contains(&"- no stored workflow steps available".to_string()));
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

    #[test]
    fn watch_category_selection_defaults_to_daily_use_categories_in_stable_order() {
        let categories = selected_watch_categories(&[]);

        assert_eq!(
            categories.into_iter().collect::<Vec<_>>(),
            vec![
                WatchCategory::Events,
                WatchCategory::Patterns,
                WatchCategory::Suggestions,
            ]
        );
    }

    #[test]
    fn watch_category_filters_include_explicit_flags_in_stable_order() {
        let categories = selected_watch_categories(&watch_category_filters(
            true,
            false,
            true,
            &[WatchCategory::Patterns],
        ));

        assert_eq!(
            categories.into_iter().collect::<Vec<_>>(),
            vec![
                WatchCategory::Events,
                WatchCategory::Patterns,
                WatchCategory::Suggestions,
            ]
        );
    }

    #[test]
    fn watch_raw_event_rendering_is_compact_and_readable() {
        let record = StoredRawEvent {
            id: 1,
            event: flow_core::events::RawEvent {
                ts: Utc::now(),
                source: flow_core::events::EventSource::FileWatcher,
                payload: serde_json::json!({
                    "kind": "rename",
                    "path": "/tmp/archive/invoice-1001.pdf",
                    "from_path": "/tmp/downloads/invoice-1001.pdf",
                }),
            },
        };

        assert_eq!(
            render_watch_raw_event(&record),
            Some(
                "[event] rename: /tmp/downloads/invoice-1001.pdf -> /tmp/archive/invoice-1001.pdf"
                    .to_string()
            )
        );
    }

    #[test]
    fn watch_suggestion_update_prefers_status_changes_over_generic_updates() {
        let previous = WatchSuggestionSnapshot {
            suggestion_id: 7,
            pattern_id: 3,
            status: "pending".to_string(),
            signature: "CreateFile:invoice".to_string(),
            canonical_summary: "CreateFile -> RenameFile".to_string(),
            proposal_text: "Rename invoices".to_string(),
        };
        let suggestion = StoredSuggestionRecord {
            suggestion_id: 7,
            pattern_id: 3,
            status: "approved".to_string(),
            signature: "CreateFile:invoice".to_string(),
            canonical_summary: "CreateFile -> RenameFile".to_string(),
            proposal_text: "Rename invoices".to_string(),
            shown_count: 0,
            accepted_count: 1,
            rejected_count: 0,
            snoozed_count: 0,
            last_shown_ts: None,
            last_accepted_ts: None,
            last_rejected_ts: None,
            last_snoozed_ts: None,
        };

        assert_eq!(
            render_watch_suggestion_updated(&previous, &suggestion),
            "[suggestion] status changed: #7 pending -> approved"
        );
    }

    #[test]
    fn watch_pattern_update_labels_strengthened_when_repetition_count_grows() {
        let previous = WatchPatternSnapshot {
            pattern_id: 1,
            signature: "CreateFile:invoice->MoveFile:invoice".to_string(),
            count: 3,
            avg_duration_ms: 12_000,
            canonical_summary: "CreateFile -> MoveFile".to_string(),
            usefulness_score: 0.8,
            last_seen_at: "2026-03-13T10:00:00Z".to_string(),
        };
        let pattern = StoredPattern {
            pattern_id: 1,
            signature: "CreateFile:invoice->MoveFile:invoice".to_string(),
            count: 4,
            avg_duration_ms: 12_000,
            canonical_summary: "CreateFile -> MoveFile".to_string(),
            usefulness_score: 0.85,
            last_seen_at: "2026-03-13T10:05:00Z".to_string(),
        };

        assert_eq!(
            render_watch_pattern_updated(&previous, &pattern),
            "[pattern] candidate strengthened: invoice_workflow (repetitions: 4)"
        );
    }

    #[test]
    fn intelligence_feedback_export_is_deterministic_and_versioned() {
        let export = build_intelligence_feedback_export(
            vec![
                stored_suggestion_for_export(1, "CreateFile:invoice-a", 0.9),
                stored_suggestion_for_export(2, "CreateFile:invoice-b", 0.8),
            ],
            &[StoredSession {
                session_id: 1,
                start_ts: "2026-01-15T09:00:00+00:00".to_string(),
                end_ts: "2026-01-15T10:00:00+00:00".to_string(),
                event_count: 4,
                duration_ms: 3_600_000,
            }],
            &LocalUsageStats {
                pattern_count: 2,
                suggestion_count: 2,
                approved_automation_count: 1,
                automation_run_count: 3,
                undo_run_count: 1,
                estimated_time_saved_ms: 20_000,
            },
            "2026-03-13T12:00:00+00:00".to_string(),
        );

        let json = serde_json::to_string_pretty(&export).unwrap();
        let value: Value = serde_json::from_str(&json).unwrap();

        assert_eq!(
            value["schema_name"],
            Value::String("flowd.intelligence_feedback_export".to_string())
        );
        assert_eq!(value["export_version"], Value::Number(1.into()));
        assert_eq!(
            value["generated_at"],
            Value::String("2026-03-13T12:00:00+00:00".to_string())
        );
        assert_eq!(value["context"]["candidate_count"], Value::Number(2.into()));
        assert_eq!(
            value["context"]["feedback_summary"]["shown_count"],
            Value::Number(6.into())
        );
        assert_eq!(
            value["suggestion_records"][0]["pattern_signature"],
            Value::String("CreateFile:invoice-a".to_string())
        );
        assert_eq!(
            value["suggestion_records"][0]["evaluation_context"]["recency"]
                ["seconds_since_last_rejected"],
            Value::Number(0.into())
        );
        assert_eq!(
            value["suggestion_records"][1]["suggestion"]["proposal_text"],
            Value::String("Proposal for CreateFile:invoice-b".to_string())
        );
    }

    #[test]
    fn intelligence_feedback_export_empty_state_is_stable() {
        let export = build_intelligence_feedback_export(
            Vec::new(),
            &[],
            &LocalUsageStats {
                pattern_count: 0,
                suggestion_count: 0,
                approved_automation_count: 0,
                automation_run_count: 0,
                undo_run_count: 0,
                estimated_time_saved_ms: 0,
            },
            "2026-03-13T12:00:00+00:00".to_string(),
        );

        assert_eq!(export.schema_name, "flowd.intelligence_feedback_export");
        assert_eq!(export.export_version, 1);
        assert_eq!(export.context.candidate_count, 0);
        assert_eq!(
            export.context.session_summary,
            InternalSessionSummary::default()
        );
        assert_eq!(
            export.context.feedback_summary,
            InternalFeedbackSummary::default()
        );
        assert!(export.suggestion_records.is_empty());
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

fn render_watch_raw_event(record: &StoredRawEvent) -> Option<String> {
    let event = &record.event;
    match event.source {
        flow_core::events::EventSource::FileWatcher => {
            let kind = event
                .payload
                .get("kind")
                .and_then(|value| value.as_str())
                .unwrap_or("event");
            let path = event
                .payload
                .get("path")
                .and_then(|value| value.as_str())
                .unwrap_or("-");
            let from_path = event
                .payload
                .get("from_path")
                .and_then(|value| value.as_str());

            match (kind, from_path) {
                ("rename", Some(from_path)) | ("move", Some(from_path)) => Some(format!(
                    "[event] {kind}: {} -> {}",
                    abbreviate_home(from_path),
                    abbreviate_home(path)
                )),
                ("create", _) => Some(format!("[event] file created: {}", abbreviate_home(path))),
                ("remove", _) | ("delete", _) => {
                    Some(format!("[event] file removed: {}", abbreviate_home(path)))
                }
                ("write" | "modify" | "access", _) => None,
                _ => Some(format!("[event] file {kind}: {}", abbreviate_home(path))),
            }
        }
        flow_core::events::EventSource::Terminal => {
            let command = event
                .payload
                .get("redacted_command")
                .and_then(|value| value.as_str())
                .unwrap_or("command");
            let kind = event
                .payload
                .get("kind")
                .and_then(|value| value.as_str())
                .unwrap_or("command");
            Some(format!("[event] terminal {kind}: {command}"))
        }
        flow_core::events::EventSource::Clipboard => None,
        flow_core::events::EventSource::Browser => match event
            .payload
            .get("kind")
            .and_then(|value| value.as_str())
            .unwrap_or("browser")
        {
            "download" => {
                let path = event
                    .payload
                    .get("path")
                    .and_then(|value| value.as_str())
                    .or_else(|| {
                        event
                            .payload
                            .get("filename")
                            .and_then(|value| value.as_str())
                    })
                    .unwrap_or("-");
                Some(format!(
                    "[event] browser download: {}",
                    abbreviate_home(path)
                ))
            }
            "visit" => {
                let url = event
                    .payload
                    .get("url")
                    .and_then(|value| value.as_str())
                    .unwrap_or("-");
                Some(format!("[event] browser visit: {url}"))
            }
            _ => None,
        },
        flow_core::events::EventSource::ActiveWindow => None,
    }
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

fn abbreviate_home(value: &str) -> String {
    let Some(home) = std::env::var_os("HOME") else {
        return value.to_string();
    };
    let home = home.to_string_lossy();

    if let Some(remainder) = value.strip_prefix(home.as_ref()) {
        if remainder.is_empty() {
            "~".to_string()
        } else if remainder.starts_with(std::path::MAIN_SEPARATOR) {
            format!("~{remainder}")
        } else {
            value.to_string()
        }
    } else {
        value.to_string()
    }
}
