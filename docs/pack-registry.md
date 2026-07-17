# Pack Registry Client

`flowctl` can install workflow packs from a local directory or from a registry index.

A registry index is a small TOML file that lists pack metadata and a relative path to each pack directory. The same format works for:

- local paths during development
- HTTPS static hosting (GitHub Pages, object storage, an internal hub)

## Index format

```toml
[registry]
name = "flowd example registry"
schema_version = 1

[[packs]]
id = "demo.rename-downloads"
name = "Demo Rename Downloads"
version = "0.1.0"
description = "Example pack with a single automation for renaming downloads."
path = "../packs/demo-pack"
```

`path` is resolved relative to the index file (or the parent URL for HTTPS indexes).

For HTTPS installs, `flowctl` downloads:

1. `{base}/{path}/workflow-pack.toml`
2. each automation file listed in the manifest

No pack contents are fetched until you install a specific pack id.

## Commands

```bash
# Local pack directory (unchanged)
flowctl packs validate ./examples/packs/demo-pack
flowctl packs install ./examples/packs/demo-pack

# Registry search / install
flowctl packs search --registry ./examples/registry/index.toml
flowctl packs search rename --registry ./examples/registry/index.toml
flowctl packs install demo.rename-downloads --registry ./examples/registry/index.toml

# HTTPS registry (opt-in; only the index and chosen pack files are fetched)
flowctl packs search --registry https://example.test/flowd/registry/index.toml
flowctl packs install demo.rename-downloads --registry https://example.test/flowd/registry/index.toml
```

Installed packs still land under the local packs directory (typically `~/.config/flowd/packs/`).

## Privacy notes

- Registry use is opt-in via `--registry`.
- Default local observation and automation stay on-device.
- Prefer a registry you control when installing packs outside the repo examples.
