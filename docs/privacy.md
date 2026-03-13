# Privacy

## Principles
- local-first by default
- explicit observed zones
- redact sensitive payloads where possible
- no cloud dependency in the open-core template
- start with reversible actions only

## Recommended defaults
- keep clipboard observation disabled unless explicitly enabled
- keep clipboard capture in metadata-only mode by default
- allow redacted clipboard previews only when explicitly configured
- allow limited plaintext clipboard previews only when clipboard redaction is disabled
- redact terminal command arguments where possible
- strip browser query strings by default
- limit observation to selected folders and sources

## Trust model
Users should be able to inspect:
- what is collected
- where it is stored
- what actions can be executed
- what safety constraints exist
