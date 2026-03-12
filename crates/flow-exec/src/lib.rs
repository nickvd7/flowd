pub mod engine;
pub mod service;

pub use engine::{
    dry_run, execute, execute_report, plan, plan_undo, ExecutionReport, PlannedOperation,
    StoredExecutionReport,
};
pub use service::{
    approve_suggestion, disable_automation, dry_run_automation, enable_automation,
    execute_automation, list_runs, preview_automation, preview_suggestion, undo_automation_run,
    AutomationPreview, DryRunOutcome, PreviewExample, PreviewRisk, UndoOutcome,
};
