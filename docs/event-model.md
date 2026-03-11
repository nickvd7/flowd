# Event Model

## Raw events
Raw events are captured from local adapters before interpretation.

Common sources:
- file watcher
- terminal hook
- clipboard
- active window
- browser bridge

Suggested fields:
- timestamp
- source
- payload JSON

## Normalized events
Normalized events convert raw inputs into a stable workflow taxonomy.

Current action types:
- OpenApp
- SwitchApp
- CopyText
- PasteText
- RunCommand
- CreateFile
- RenameFile
- MoveFile
- VisitUrl
- DownloadFile

## Design goal
Keep the normalized vocabulary small and deterministic so downstream pattern detection stays stable and testable.
