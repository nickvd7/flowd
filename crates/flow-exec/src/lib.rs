pub mod engine;
pub mod service;

pub use engine::{dry_run, execute, plan, ExecutionReport, PlannedOperation};
pub use service::{
    approve_suggestion, disable_automation, dry_run_automation, enable_automation,
    execute_automation, DryRunOutcome,
};
