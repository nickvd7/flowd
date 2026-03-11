use anyhow::Result;
use flow_dsl::AutomationSpec;

pub fn dry_run(spec: &AutomationSpec) -> Result<Vec<String>> {
    let mut preview = vec![format!("trigger: {}", spec.trigger.r#type)];
    for action in &spec.actions {
        preview.push(format!("action: {:?}", action));
    }
    Ok(preview)
}
