# Source this file from .zshrc after linking and building the plugin.

autoload -Uz add-zsh-hook

_herdr_tree_resolve() {
  [[ -n "${_HERDR_TREE_BIN:-}" ]] && return

  local herdr="${HERDR_BIN_PATH:-herdr}"
  local config_dir="${HERDR_TREE_CONFIG_DIR:-}"
  if [[ -z "$config_dir" ]]; then
    config_dir="$($herdr plugin config-dir herdr.tree 2>/dev/null)" || return
  fi
  local paths_file="$config_dir/runtime-paths"
  [[ -r "$paths_file" ]] || return

  local -a runtime_paths
  runtime_paths=("${(@f)$(<"$paths_file")}")
  [[ ${#runtime_paths} -ge 2 ]] || return
  typeset -g _HERDR_TREE_BIN="${HERDR_TREE_BIN:-${runtime_paths[1]}}"
  typeset -g _HERDR_TREE_CONFIG_DIR="$config_dir"
  typeset -g _HERDR_TREE_STATE_DIR="${HERDR_TREE_STATE_DIR:-${runtime_paths[2]}}"
}

_herdr_tree_wake() {
  [[ -n "${HERDR_ENV:-}" ]] || return
  _herdr_tree_resolve
  [[ -x "${_HERDR_TREE_BIN:-}" ]] || return
  HERDR_PLUGIN_CONFIG_DIR="$_HERDR_TREE_CONFIG_DIR" \
    HERDR_PLUGIN_STATE_DIR="$_HERDR_TREE_STATE_DIR" \
    "$_HERDR_TREE_BIN" wake >/dev/null 2>&1 &!
}

add-zsh-hook precmd _herdr_tree_wake
add-zsh-hook chpwd _herdr_tree_wake

