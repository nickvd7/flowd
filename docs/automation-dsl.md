# Automation DSL

The internal DSL represents safe, reviewable automations.

## Initial scope
Only support:
- file rename
- file move
- dry-run preview
- undo-log compatible actions

## Example

```yaml
id: auto_invoice_sort
trigger:
  type: file_created
  path: ~/Downloads
actions:
  - type: Rename
    template: "{date}_{original}"
  - type: Move
    destination: ~/Documents/Invoices
safety:
  dry_run_first: true
  undo_log: true
```

## Rules
- no delete actions in v1
- no arbitrary shell execution in v1
- every executable action should be inspectable and reversible where possible
