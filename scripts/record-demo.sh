#!/usr/bin/env bash
# Record the real zsh adapter with VHS, without reading the user's shell or goh data.
#
#   scripts/record-demo.sh
#
# docs/media/demo.tape → docs/media/features.gif (README hero: menus, Tab, typo, warning, folder tree)
# docs/media/tour.tape → docs/media/demo.gif     (project scripts and targets, snippets, history, masked secrets)
#
# Each tape gets its own disposable home, Git repo, fixtures and private daemon, removed even on failure.
set -euo pipefail
cd "$(dirname "$0")/.."
demo_root=$PWD

for tool in cargo vhs ttyd ffmpeg ffprobe git zsh; do
  command -v "$tool" >/dev/null || { printf 'Missing dependency: %s\n' "$tool" >&2; exit 1; }
done

host_zsh=$(command -v zsh)
cargo build --release --locked -p goh-daemon
mkdir -p target

demo_dir=
cleanup() {
  [ -n "$demo_dir" ] || return 0
  GOH_SOCKET="$demo_dir/s" "$demo_root/target/release/goh" stop >/dev/null 2>&1 || true
  rm -rf "$demo_dir"
  demo_dir=
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM HUP

# A fresh demo environment in $demo_dir. $1 is `on` to let goh read the fixture history (GOH_RECORD).
setup() {
  # macOS Unix sockets have a short path limit; do not use its long default TMPDIR.
  demo_dir=$(mktemp -d /tmp/goh-demo.XXXXXX)
  mkdir -p "$demo_dir/"{bin,vhs,home,project,specs,cache,config,data,state}
  export GOH_DEMO_DIR="$demo_dir" GOH_DEMO_ROOT="$demo_root" GOH_DEMO_RECORD="$1" GOH_DEMO_ZSH="$host_zsh"

  # Only terminal children get this clean environment. VHS retains access to the
  # host's installed fonts and browser; no user configuration enters the recording.
  cat > "$demo_dir/bin/goh-demo-shell" <<'SH'
#!/bin/sh
if [ "$#" -eq 0 ]; then set -- "$GOH_DEMO_ZSH" -d -i; fi
exec env -i \
  PATH="$GOH_DEMO_ROOT/target/release:$PATH" \
  HOME="$GOH_DEMO_DIR/home" ZDOTDIR="$GOH_DEMO_DIR/home" \
  XDG_CONFIG_HOME="$GOH_DEMO_DIR/config" XDG_CACHE_HOME="$GOH_DEMO_DIR/cache" \
  XDG_DATA_HOME="$GOH_DEMO_DIR/data" XDG_STATE_HOME="$GOH_DEMO_DIR/state" \
  LANG=en_US.UTF-8 LC_ALL=en_US.UTF-8 TERM=xterm-256color \
  GIT_CONFIG_NOSYSTEM=1 GIT_CONFIG_GLOBAL=/dev/null \
  GOH_DEMO_ROOT="$GOH_DEMO_ROOT" GOH_DEMO_DIR="$GOH_DEMO_DIR" \
  GOH_CTL="$GOH_DEMO_ROOT/target/release/goh" \
  GOH_SOCKET="$GOH_DEMO_DIR/s" GOH_CONFIG="$GOH_DEMO_DIR/config.json" \
  GOH_USAGE="$GOH_DEMO_DIR/usage" GOH_VISITS="$GOH_DEMO_DIR/visits" \
  GOH_RUNS="$GOH_DEMO_DIR/runs" GOH_LOG="$GOH_DEMO_DIR/log" \
  GOH_SHELL_HISTORY="$GOH_DEMO_DIR/history" \
  GOH_UPDATE_STATE="$GOH_DEMO_DIR/update" GOH_SPEC_DIR="$GOH_DEMO_DIR/specs" \
  GOH_DISPLAY=inline GOH_INLINE_STYLE=box GOH_LIVE=on GOH_LIMIT=5 \
  GOH_ICONS=unicode GOH_RECORD="$GOH_DEMO_RECORD" GOH_PREDICT=off GOH_UPDATE_CHECK=off \
  "$@"
SH
  # VHS only starts shells it knows, so its `zsh` is this one: the clean demo shell, with
  # VHS's own zsh arguments dropped and this folder taken off PATH again.
  cat > "$demo_dir/vhs/zsh" <<'SH'
#!/bin/sh
PATH=${PATH#"$GOH_DEMO_DIR/vhs:"} exec "$GOH_DEMO_DIR/bin/goh-demo-shell"
SH
  chmod +x "$demo_dir/bin/goh-demo-shell" "$demo_dir/vhs/zsh"

  printf '%s\n' '{"configVersion":1,"privacy":{"learnChoices":false},"completion":{"autoSpecs":false}}' > "$demo_dir/config.json"
  # snippets.json sits next to the config file.
  printf '%s\n' '[{"name":"gcm","template":"git commit -m \"{{cursor}}\"","description":"Commit with a message"}]' \
    > "$demo_dir/snippets.json"
  # Read-only zsh history for the tour; the token is a placeholder that goh shows as ****.
  cat > "$demo_dir/history" <<'HIST'
: 1700000000:0;docker compose logs -f web
: 1700000100:0;export API_TOKEN=demo-0000-not-a-real-token
: 1700000200:0;docker compose up -d
: 1700000300:0;docker compose up -d
HIST
  printf '%s\n' 'unsetopt GLOBAL_RCS' > "$demo_dir/home/.zshenv"
  cat > "$demo_dir/home/.zshrc" <<'ZSH'
bindkey -e
HISTFILE=/dev/null
SAVEHIST=0
PROMPT='%F{blue}%1~%f %F{cyan}❯%f '
RPROMPT=
cd "$GOH_DEMO_DIR/project" || exit 1
source "$GOH_DEMO_ROOT/adapters/zsh/goh.zsh" || exit 1
clear
ZSH

  # A real project: Git branches, npm scripts, Make targets and a source tree, all read by goh.
  local p="$demo_dir/project"
  mkdir -p "$p/src/components" "$p/tests"
  : > "$p/src/main.ts"; : > "$p/src/server.ts"; : > "$p/tests/app.test.ts"
  : > "$p/src/components/Button.tsx"; : > "$p/src/components/Header.tsx"
  printf '%s\n' '{"name":"web","private":true,"scripts":{"dev":"vite","build":"vite build","test":"vitest","lint":"eslint ."}}' \
    > "$p/package.json"
  printf 'build:\n\tnpm run build\ntest:\n\tnpm test\nrelease: build test\n\t./scripts/release.sh\n' > "$p/Makefile"
  demo_run git -C "$p" -c init.defaultBranch=main init -q
  demo_run git -C "$p" add -A
  demo_run git -C "$p" -c user.name='Demo Developer' -c user.email=demo@example.invalid \
    -c commit.gpgsign=false -c core.hooksPath=/dev/null commit -qm 'Initial commit'
  demo_run git -C "$p" branch feature/auth
  demo_run git -C "$p" branch feature/search
  demo_run goh start >/dev/null
}
demo_run() { "$demo_dir/bin/goh-demo-shell" "$@"; }

# Record $1 into $2 and check it before anything is published.
record() {
  PATH="$demo_dir/vhs:$PATH" vhs -o "$demo_dir/out.gif" "$1"
  # VHS encodes a palette GIF with frame deltas; disabling cursor blink also avoids
  # spending frames/bytes on idle changes.
  local width duration bytes
  width=$(ffprobe -v error -select_streams v:0 -show_entries stream=width -of csv=p=0 "$demo_dir/out.gif")
  duration=$(ffprobe -v error -show_entries format=duration -of csv=p=0 "$demo_dir/out.gif")
  bytes=$(wc -c < "$demo_dir/out.gif" | tr -d ' ')
  [ "$width" = 1100 ] || { printf '%s: unexpected GIF width %s\n' "$1" "$width" >&2; exit 1; }
  awk -v d="$duration" 'BEGIN { exit !(d >= 10 && d <= 20) }' || {
    printf '%s: GIF duration must be 10–20 seconds, got %s\n' "$1" "$duration" >&2; exit 1;
  }
  [ "$bytes" -le 3000000 ] || { printf '%s: GIF exceeds the 3 MB budget: %s bytes\n' "$1" "$bytes" >&2; exit 1; }
  recorded="$2 ($duration s, $bytes bytes)"
}

setup off
record docs/media/demo.tape features.gif
# Enter ran the line goh built: the fixture repository must now be on the picked branch.
[ "$(demo_run git -C "$demo_dir/project" branch --show-current)" = feature/search ] || {
  printf 'The demo did not switch to the selected branch.\n' >&2; exit 1;
}
mv "$demo_dir/out.gif" target/features.gif
printf 'Recorded %s\n' "$recorded"
cleanup

setup on
record docs/media/tour.tape demo.gif
mv "$demo_dir/out.gif" target/demo.gif
printf 'Recorded %s\n' "$recorded"
cleanup

# Publish only when both recordings are complete.
cp target/features.gif docs/media/features.gif
cp target/demo.gif docs/media/demo.gif
printf 'Updated docs/media/features.gif and docs/media/demo.gif\n'
