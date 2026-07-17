pub mod engine;
pub mod service;

pub use engine::{
    dry_run, execute, execute_for_path, execute_report, file_matches_trigger, plan, plan_for_path,
    plan_undo, ExecutionReport, PlannedOperation, StoredExecutionReport,
};
pub use service::{
    approve_suggestion, disable_automation, dry_run_automation, enable_automation,
    execute_automation, execute_automation_for_path, execute_automation_with_force,
    format_run_audit_summary, get_run, list_runs, preview_automation, preview_suggestion,
    teach_from_session, undo_automation_run, AutomationPreview, DryRunOutcome, ExecutionOutcome,
    PreviewExample, PreviewRisk, UndoOutcome,
};
