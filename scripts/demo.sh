#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
root_dir="$(cd -- "${script_dir}/.." && pwd)"
session_name="${1:-flux-demo}"

if ! command -v tmux >/dev/null 2>&1; then
  printf 'tmux is required to run the demo layout\n' >&2
  exit 1
fi

if tmux has-session -t "${session_name}" 2>/dev/null; then
  printf "tmux session '%s' already exists\n" "${session_name}" >&2
  exit 1
fi

tmux new-session -d -s "${session_name}" -c "${root_dir}"
tmux send-keys -t "${session_name}:0.0" "cargo run -p flux-server" C-m
tmux split-window -h -t "${session_name}:0.0" -c "${root_dir}"
tmux send-keys -t "${session_name}:0.1" "cargo run -p flux-producer" C-m
tmux select-pane -t "${session_name}:0.0"
tmux split-window -v -f -p 38 -t "${session_name}:0.0" -c "${root_dir}"
tmux send-keys -t "${session_name}:0.2" "cargo run -p flux-dashboard" C-m

if [[ -n "${TMUX:-}" ]]; then
  tmux switch-client -t "${session_name}"
else
  tmux attach-session -t "${session_name}"
fi
