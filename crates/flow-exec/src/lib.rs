pub mod engine;
pub mod service;

pub use engine::{dry_run, execute, plan, ExecutionReport, PlannedOperation};
pub use service::{approve_suggestion, dry_run_automation, execute_automation, DryRunOutcome};
