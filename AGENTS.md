# Project context

This repository is a Rust-based Herdr plugin that projects workspaces, tabs,
agent lifecycle state, and Git state into Herdr's native expanded Spaces
sidebar. A resident daemon owns runtime state, subscribes to lifecycle events,
periodically refreshes Git, and publishes workspace metadata tokens. It does
not implement a separate sidebar or TUI.

## Rust module organization

Organize code by cohesive responsibility, not file size. Keep Herdr wire types,
projected domain types, rendering, persistence, Git inspection, managed config,
and daemon orchestration separated.

Keep related types, private helpers, and unit tests together. Split modules
when responsibilities differ, but avoid one-function-per-file fragmentation.
Prefer domain names over generic `utils`, `helpers`, or `common`; keep `mod.rs`
focused on declarations and re-exports.

Use `thiserror` at module boundaries and `anyhow` only for executable
orchestration. Prefer borrowed inputs in projection and formatting code. Own
data at snapshot, cache, persistence, and outbound metadata boundaries. Avoid
`unwrap` and `expect` in production code.

## Local decisions and invariants

- Treat Herdr lifecycle events and shell notifications as wake hints. A fresh
  `session.snapshot` remains authoritative; do not incrementally reconstruct a
  session from events.
- Subscribe only to session-wide event kinds. Herdr rejects an entire
  `events.subscribe` request when one entry is malformed, and the pane-scoped
  kinds require a concrete `pane_id`. A short snapshot poll, not the Git tick,
  covers what no session-wide event reports.
- The service loop owns mutable runtime state. Worker threads may deliver
  messages, but they must not independently mutate the current tree or token
  cache.
- Scope daemon control sockets by the Herdr socket path. Named sessions share
  the plugin state directory but require independent daemons and controls.
- Animation may only re-render cached state. A 150 ms spinner or blocked pulse
  must never trigger a snapshot, Git process, state write, or config reload.
- Keep workspace metadata patches at 16 tokens or fewer and every configured
  sidebar row at 16 cells or fewer. Preserve monotonically increasing metadata
  sequence numbers across daemon restarts.
- Render a workspace root as disclosure arrow, Herdr's workspace number, and
  display name. Render a tab row from its display name alone; Herdr's own tab
  labels already carry the tab number. Keep the projected tab index
  workspace-local and one-based for diagnostics. Select a tab's layout-focused
  tiled pane, then its first tiled pane, then its first pane record. Pane counts
  exclude popups and remain stable while zoomed.
- Preserve explicit `workspace.renamed` and `tab.renamed` values as manual
  display names. Capture a workspace's first observed directory once;
  subsequent cwd changes must not rename it or overwrite a manual name.
- Aggregate multi-agent lifecycle state in this order:
  `blocked > working > done > idle > unknown > no-agent`. Keep `NoAgent`
  distinct internally and render it with its own configured glyph while it
  shares the idle row token and color.
- Hold done state according to configuration and acknowledge it on tab focus.
  Hold blocked state until work resumes when enabled. Persist holds needed to
  survive daemon restarts.
- Git status comes from bounded porcelain-v2 processes with branch and stash
  headers. Cache by repository, preserve the last successful value after a
  transient failure, and never run Git on the animation path.
- Load and validate configuration before changing metadata or Herdr config.
  Keep Rust defaults, `config.example.toml`, and the README table synchronized;
  unknown keys remain errors.
- Managed Herdr configuration is explicit, fenced, backed up, atomically
  replaced, and reversible. Refuse foreign `[ui.sidebar.spaces]` or
  `[theme.custom]` ownership instead of merging it silently.
- State transitions stay under the exclusive lock and save through atomic file
  replacement. Malformed state must be reported and left untouched.
- Never read terminal contents, transcripts, prompt text, command arguments,
  session data, or arbitrary pane environments. Shell hooks send only a wake
  signal and never edit shell startup files.
- Automated tests must not mutate a live Herdr session. Socket integration
  tests use a fake Unix socket. Manifest and config validation use isolated XDG
  directories and a nonexistent `HERDR_SOCKET_PATH`.
- Keep `README.md` synchronized when behavior, requirements, configuration,
  actions, setup, cleanup, validation, or known limitations change. Keep its
  single-line section index aligned with its headings.
- Update `AGENTS.md` when repository-wide invariants, architecture boundaries,
  authoritative commands, or source-of-truth references change.

## Validation

- Rust changes: `cargo fmt --all -- --check`, then
  `cargo test --all-targets --locked`, then
  `cargo clippy --all-targets --all-features --locked -- -D warnings`.
- Rustdoc changes: `cargo test --doc --locked` and
  `RUSTDOCFLAGS='-D warnings' cargo doc --no-deps --document-private-items --locked`.
- Zsh hook changes: `zsh -n shell/herdr-tree.zsh`.
- Release changes: `cargo build --release --locked`.
- Manifest changes: validate only with an isolated Herdr home; never link the
  working tree into the user's live registry as an automated check:

  ```sh
  validation_root=$(mktemp -d)
  mkdir -p "$validation_root/config/herdr"
  touch "$validation_root/config/herdr/config.toml"
  XDG_CONFIG_HOME="$validation_root/config" \
    XDG_DATA_HOME="$validation_root/data" \
    XDG_STATE_HOME="$validation_root/state" \
    XDG_CACHE_HOME="$validation_root/cache" \
    HERDR_SOCKET_PATH="$validation_root/no-server.sock" \
    herdr plugin link . --disabled
  ```

## References

- User behavior, setup, actions, cleanup, and limitations: `README.md`
- Configuration defaults and editable schema: `config.example.toml`
- Plugin declaration and supported manifest hooks: `herdr-plugin.toml`
- Shell wake integration: `shell/herdr-tree.zsh`
