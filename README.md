# Herdr Tree

Herdr Tree turns Herdr's native expanded Spaces sidebar into a live workspace
and tab tree. It shows detected agents, lifecycle state, tiled pane counts, and
Git state without reading terminal contents or replacing Herdr's renderer.

[Behavior](#behavior) · [Status and Git indicators](#status-and-git-indicators) · [Requirements](#requirements) · [Local installation](#local-installation) · [Configuration](#configuration) · [Zsh integration](#zsh-integration) · [Actions and keybindings](#actions-and-keybindings) · [Current limitations](#current-limitations) · [Development](#development) · [Unconfigure and unlink](#unconfigure-and-unlink)

## Behavior

The expanded sidebar keeps Herdr's native workspace navigation while rendering
each workspace as a collapsible tree root and its tabs as children:

```text
▾ 1 dashboard
   ⣟ codex - 1 backend (3)
       feature/auth ↑2 ▣1 ✱4
   ✓ claude - 2 frontend
       main
   ! pi - 3 config
       feature/config ↓1
   ○ 4 shell
       main

▸ 2 billing (4)
```

Each workspace root shows its disclosure arrow, Herdr's workspace number, and
the workspace name. Tab rows show Herdr's own tab label unchanged, so a label
that already carries the tab number is not numbered twice. The selected layout
pane provides the directory and detected agent; the first tiled pane is the
stable fallback. Pane counts exclude popups and follow `show_pane_count`.

The active native workspace uses Herdr's `active_row_bg`. The active synthetic
tab uses bold text and the configured accent foreground. Explicit Herdr
workspace and tab renames become persistent manual display names. Collapse,
manual-name ownership, and held lifecycle states survive daemon restarts.
For an automatic workspace name, the first observed directory basename is
captured once; changing a pane's cwd later does not rename the workspace root.

The daemon subscribes to Herdr's session-wide lifecycle events and treats them
as wake hints, so creating, closing, renaming, or focusing a workspace, tab, or
pane is reflected within a few milliseconds. A one-second snapshot poll catches
whatever no session-wide event covers, such as agent status changes, which
Herdr reports only to per-pane subscribers. Fresh snapshots remain
authoritative. Git refreshes on its own interval, shell hooks request
prompt/cwd reconciliation, and spinner frames only re-render cached metadata.

## Status and Git indicators

Agent lifecycle glyphs:

| Glyph | Meaning |
| --- | --- |
| `⣷ ⣯ ⣟ ⡿ ⢿ ⣻ ⣽ ⣾` | Working animation, advancing every 150 ms by default. |
| `✓` | Done; held until the tab is focused by default. |
| `!` / `·` | Blocked pulse; held until work resumes by default. |
| `◉` | Idle; an agent is open on the tab. |
| `○` | No detected agent. |
| `◌` | Agent state is unknown. |

For multiple agents in one tab, state precedence is `blocked > working > done
> idle > unknown > no-agent`; distinct agent names are shown in pane order.

Git indicators:

| Glyph | Meaning |
| --- | --- |
| `↑N` | Commits ahead of upstream. |
| `↓N` | Commits behind upstream. |
| `↕B/A` | Diverged: behind `B`, ahead `A`. |
| `▣N` | Stash entries. |
| `✱N` | Dirty paths in compact mode; unstaged paths in detailed mode. |
| `●N` | Staged paths in detailed mode. |
| `?N` | Untracked paths in detailed mode. |
| `!N` | Conflicted paths. |
| `@abc1234` | Detached `HEAD`. |

## Requirements

- Herdr 0.9.0 or newer.
- Rust 1.85 or newer for local builds.
- Linux or macOS. Windows is not currently declared as supported.
- Optional Zsh integration for immediate cwd and prompt-triggered refreshes.

## Local installation

Build first, because `herdr plugin link` validates and registers a local plugin
but does not run the manifest build command:

```bash
cargo build --release --locked
herdr plugin link "$PWD"
herdr plugin action list --plugin herdr.tree
```

Configuration is deliberately explicit. Install the managed sidebar and theme
blocks, then start and seed the daemon:

```bash
herdr plugin action invoke herdr.tree.configure
herdr plugin action invoke herdr.tree.refresh
```

The configure action creates a timestamped backup of Herdr's `config.toml`,
inserts clearly fenced blocks, validates ownership conflicts, writes atomically,
and reloads Herdr. It refuses an existing hand-written `[ui.sidebar.spaces]` or
`[theme.custom]` table rather than overwriting it.

`herdr-radar` owns those same tables, so unconfigure it before configuring
Herdr Tree. `herdr-title-name` can remain enabled, but its native tab labels may
duplicate directory or agent text shown by the tree.

Startup hooks run when a Herdr server starts, not when a plugin is linked or
enabled. The explicit refresh above starts the daemon immediately. Inspect
plugin command failures with:

```bash
herdr plugin log list --plugin herdr.tree
```

## Configuration

On first invocation, the plugin seeds `config.toml` in its Herdr-managed
configuration directory:

```bash
herdr plugin config-dir herdr.tree
```

Configuration is strict: unknown keys, invalid colors, empty glyphs, invalid
intervals, or a row capacity beyond Herdr's limits fail before metadata or
global config is changed. The complete editable example is
[`config.example.toml`](config.example.toml).

| Setting | Default | Effect |
| --- | --- | --- |
| `layout` | `detailed` | `detailed` uses primary and Git rows; `compact` uses one row per tab. |
| `max_tabs` | `7` | Visible slots per workspace; at most 7 detailed or 14 compact. |
| `show_pane_count` | `multiple` | Accepts `never`, `always`, or `multiple`. |
| `workspace_tab_count` | `collapsed` | Shows totals `never`, when `collapsed`, or `always`. |
| `git_detail` | `compact` | Aggregate dirty paths or separate detailed categories. |
| `git_refresh_interval_seconds` | `2` | Refresh age for focused or working tabs. |
| `git_background_refresh_interval_seconds` | `5` | Refresh age for background tabs. |
| `git_command_timeout_seconds` | `3` | Kills a stalled Git subprocess while retaining cached state. |
| `spinner_interval_ms` | `150` | Working animation interval. |
| `done_hold` | `until_seen` | Accepts `until_seen`, `seconds`, or `off`. |
| `done_hold_seconds` | `6` | Hold duration when `done_hold = "seconds"`. |
| `blocked_hold` | `true` | Retains blocked state until work resumes. |
| `idle_grace_seconds` | `0.5` | Avoids false completion during short detection gaps. |

The `[glyphs]` table customizes lifecycle marks and spinner frames. The
`[colors]` table customizes tab/status foregrounds and Herdr's active workspace
and selection backgrounds. Colors accept strict `#RGB` or `#RRGGBB` values.

## Zsh integration

Herdr events and periodic refreshes are sufficient for correctness. The Zsh
hook makes cwd and prompt-related Git changes appear sooner without delaying
the prompt.

Run refresh once so the plugin seeds its runtime paths, then source the hook
from `.zshrc`:

```zsh
source /absolute/path/to/herdr-tree/shell/herdr-tree.zsh
```

The hook resolves the linked or installed binary through Herdr's plugin config
directory. It no-ops outside Herdr, sends only an asynchronous wake signal,
does not transmit command text, and never edits shell startup files.

## Actions and keybindings

The manifest provides these actions:

- `configure` installs or refreshes managed sidebar and theme blocks.
- `unconfigure` stops the daemon, clears owned metadata when possible, and
  removes only Herdr Tree's managed blocks.
- `refresh` starts the daemon if needed and requests a fresh snapshot and Git
  reconciliation.
- `toggle-collapse` collapses or expands the focused workspace.
- `expand-all` and `collapse-all` change every workspace in the session.
- `doctor` prints safe snapshot inputs and computed tokens without reading
  terminal contents, transcripts, or environment secrets.

Invoke an action directly:

```bash
herdr plugin action invoke herdr.tree.doctor
```

Or bind one in Herdr's configuration:

```toml
[[keys.command]]
key = "prefix+ctrl+c"
type = "plugin_action"
command = "herdr.tree.toggle-collapse"
description = "collapse or expand workspace tree"
```

The same action toggles in both directions: it collapses an expanded workspace
and expands a collapsed workspace.

## Current limitations

- Custom metadata-token rows support foreground color, bold, and dim styling,
  but not a per-tab background or true alpha/RGBA color. Active tabs therefore
  use an accent foreground and bold text.
- Configured rows apply only to the expanded desktop Spaces sidebar. Collapsed
  and mobile layouts retain Herdr's native compact presentation.
- Rendered rows are informational and carry no click actions of their own.
  Herdr assigns every custom Space row the workspace hit target, so clicking a
  row selects its workspace. Collapse and expand are reached through the
  `toggle-collapse` action or a keybinding.
- Operation labels such as merge, rebase, cherry-pick, revert, and bisect are
  not currently shown.
- Bash and Fish wake hooks are not included.
- Terminal contents, AI task titles, session identifiers, and transcripts are
  intentionally never inspected or displayed.

## Development

The binary entry point only reports top-level failures. Command parsing,
configuration, daemon control, Herdr socket types, tree projection, lifecycle
retention, Git parsing/cache, rendering, managed config, and persistent state
are separated into focused modules under `src/`.

Run the authoritative checks before handing off changes:

```bash
cargo fmt --all -- --check
cargo test --all-targets --locked
cargo clippy --all-targets --all-features --locked -- -D warnings
cargo test --doc --locked
RUSTDOCFLAGS='-D warnings' cargo doc --no-deps --document-private-items --locked
zsh -n shell/herdr-tree.zsh
cargo build --release --locked
```

The socket integration test uses a fake Herdr endpoint. Manifest and managed
config checks must use isolated XDG directories and must never mutate a live
Herdr registry or session.

## Unconfigure and unlink

Remove owned sidebar configuration and live metadata before unregistering the
checkout:

```bash
herdr plugin action invoke herdr.tree.unconfigure
herdr plugin unlink herdr.tree
```

Unlinking does not delete the checkout. Herdr preserves plugin configuration
and state, so a later relink can resume collapse, rename, and lifecycle holds.
