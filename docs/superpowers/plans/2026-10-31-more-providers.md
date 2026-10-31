# More Value Providers Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Suggest live values where people type them most: Docker networks and volumes, tmux sessions and windows, systemd units, and running processes for `kill`, `pkill` and `killall`.

**Architecture:** goh never runs scripts from specs (`crates/goh-core/src/spec.rs:67`); dynamic values come from a fixed list of read-only providers in the daemon. This plan adds seven variants to `Provider`, gives each a fixed argument list and a validating parser in `crates/goh-daemon/src/providers.rs` (the same background-asked path Docker uses: 120 ms per keystroke, the rest in the background), and points spec arguments at them through `tools/import-specs/enhance.mjs` so later imports keep the wiring.

**Tech Stack:** Rust (`goh-core`, `goh-daemon`), Node importer for the spec data.

**Spec:** `docs/features/project-context.md`, `docs/schemas/completion.schema.json`.

## Global Constraints

- Every user-visible change updates its spec in `docs/features/` in the same commit series: text, `updated: <today>`, and a `- unreleased: ...` line under `## Changes`. Run `scripts/check-feature-specs.sh` before the last commit.
- Conventional Commits; end every commit message with `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.
- No shell, no spec text and no buffer text reaches a subprocess: each provider is a fixed program with fixed arguments.
- Every value from a program is validated: no control characters, no leading `-`, at most 256 bytes, at most 5000 rows (10000 for processes).
- A missing or failing program yields no values (fail-open), never an error in the menu.
- Runs after `2026-10-31-more-bundled-specs.md` (it imports the `kill`, `pkill` and `killall` specs this plan wires up).

---

## File Structure

| File | Change |
|---|---|
| `crates/goh-core/src/spec.rs` | Seven `Provider` variants |
| `docs/schemas/completion.schema.json` | Seven names in both provider enums |
| `crates/goh-daemon/src/providers.rs` | Programs, arguments and parsers for the new providers |
| `tools/import-specs/enhance.mjs` | Arguments pointed at the providers |
| `crates/goh-core/specs/{docker,tmux,systemctl,kill,pkill,killall}.json` | Provider fields on those arguments |
| `crates/goh-core/src/bundled.rs` | Engine test: each argument gets its provider's values |
| `docs/features/project-context.md` | What is suggested where |

---

### Task 1: Provider variants and their daemon queries

**Files:**
- Modify: `crates/goh-core/src/spec.rs:67-125`
- Modify: `docs/schemas/completion.schema.json:32-33`
- Modify: `crates/goh-daemon/src/providers.rs`

**Interfaces:**
- Produces: `Provider::{DockerNetworks, DockerVolumes, TmuxSessions, TmuxWindows, SystemdUnits, Processes, ProcessNames}`, serialised in specs as `docker-networks`, `docker-volumes`, `tmux-sessions`, `tmux-windows`, `systemd-units`, `processes`, `process-names`. `Providers::values(provider, cwd)` returns their values. Each `Suggestion` has `name` (what is inserted) and a `description`:
  - networks, volumes: name; driver
  - tmux sessions: `work`; `3 windows`, plus ` attached` when attached
  - tmux windows: `work:1`; the window's name
  - systemd units: `nginx.service`; `active · <description>`
  - processes: pid `4242`; process name
  - process names: `node`; `process`

- [ ] **Step 1: Write the failing tests**

In `crates/goh-daemon/src/providers.rs`, inside `mod tests`:

```rust
    #[test]
    fn networks_volumes_sessions_units_and_processes_are_parsed_and_validated() {
        let nets = parse(Provider::DockerNetworks, "{\"Name\":\"bridge\",\"Driver\":\"bridge\"}\n{\"Name\":\"-bad\",\"Driver\":\"x\"}\n");
        assert_eq!(nets.iter().map(|s| (s.name.as_str(), s.description.as_deref())).collect::<Vec<_>>(), [("bridge", Some("bridge"))]);
        assert_eq!(parse(Provider::DockerVolumes, "{\"Name\":\"pgdata\",\"Driver\":\"local\"}\n")[0].name, "pgdata");

        let sessions = parse(Provider::TmuxSessions, "work\x1f3 windows attached\n-x\x1f1 windows\nwork\x1fagain\nbell\x07\x1f1 windows\n");
        assert_eq!(sessions.iter().map(|s| (s.name.as_str(), s.description.as_deref().unwrap())).collect::<Vec<_>>(), [("work", "3 windows attached")]);
        assert_eq!(parse(Provider::TmuxWindows, "work:1\x1fvim\n")[0].name, "work:1");

        let units = parse(Provider::SystemdUnits, "nginx.service loaded active running A high performance web server\nnot-a-unit\nssh.socket loaded inactive dead OpenBSD Secure Shell socket\n");
        assert_eq!(units.iter().map(|s| s.name.as_str()).collect::<Vec<_>>(), ["nginx.service", "ssh.socket"]);
        assert_eq!(units[0].description.as_deref(), Some("active · A high performance web server"));

        let ps = "  1 /sbin/launchd\n 812 /Applications/Google Chrome.app/Contents/MacOS/Google Chrome\n 900 zsh\n 901 zsh\nabc nope\n 902 -zsh\n";
        assert_eq!(
            parse(Provider::Processes, ps).iter().map(|s| (s.name.as_str(), s.description.as_deref().unwrap())).collect::<Vec<_>>(),
            [("1", "launchd"), ("812", "Google Chrome"), ("900", "zsh"), ("901", "zsh")]
        );
        assert_eq!(parse(Provider::ProcessNames, ps).iter().map(|s| s.name.as_str()).collect::<Vec<_>>(), ["launchd", "Google Chrome", "zsh"]);
    }

    #[test]
    fn tmux_sessions_are_asked_from_tmux() {
        let dir = tempfile::tempdir().unwrap();
        let tmux = script(dir.path(), "[ \"$1\" = list-sessions ] || exit 1\nprintf 'work\\037%s\\n' '3 windows'");
        let providers = Providers { tmux, ..Providers::default() };
        let deadline = Instant::now() + Duration::from_secs(5);
        while providers.values(Provider::TmuxSessions, Path::new("/")).is_empty() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(20));
        }
        assert_eq!(providers.values(Provider::TmuxSessions, Path::new("/"))[0].name, "work");
    }

    #[test]
    fn a_missing_tmux_systemctl_or_ps_gives_nothing() {
        let providers = Providers {
            tmux: PathBuf::from("/nonexistent/tmux"),
            systemctl: PathBuf::from("/nonexistent/systemctl"),
            ps: PathBuf::from("/nonexistent/ps"),
            ..Providers::default()
        };
        for p in [Provider::TmuxSessions, Provider::TmuxWindows, Provider::SystemdUnits, Provider::Processes, Provider::ProcessNames] {
            assert!(providers.values(p, Path::new("/")).is_empty(), "{p:?}");
        }
    }
```

- [ ] **Step 2: Run them to see them fail**

Run: `cargo test -p goh-daemon --lib providers::tests`
Expected: compile errors: no variant `Provider::DockerNetworks`, no field `tmux`.

- [ ] **Step 3: Add the variants**

In `crates/goh-core/src/spec.rs`, at the end of `enum Provider` (after `NodeBins,`):

```rust
    /// Docker networks (`docker network rm`, `connect`, `inspect`).
    DockerNetworks,
    /// Docker volumes (`docker volume rm`, `inspect`).
    DockerVolumes,
    /// tmux sessions of the default server (`tmux attach-session -t`, `kill-session -t`).
    TmuxSessions,
    /// tmux windows as `session:index` (`tmux select-window -t`).
    TmuxWindows,
    /// systemd units, loaded or not (`systemctl restart`, `status`, `enable`).
    SystemdUnits,
    /// Running processes by pid, described by their name (`kill`).
    Processes,
    /// Names of running processes (`pkill`, `killall`).
    ProcessNames,
```

In `docs/schemas/completion.schema.json`, append to both enum arrays (the `provider` one and the `providers` items one), after `"node-bins"`:

```json
, "docker-networks", "docker-volumes", "tmux-sessions", "tmux-windows", "systemd-units", "processes", "process-names"
```

- [ ] **Step 4: Programs, arguments and routing**

In `crates/goh-daemon/src/providers.rs`, update the module doc's first line to:

```rust
//! Fixed read-only Docker, git (changed files, commits), kubectl (pods, namespaces), tmux, systemctl, ps, zoxide and
//! directory-history queries. No shell, spec scripts, or buffer text reaches a subprocess.
```

Add three fields to `struct Providers` after `kubectl: PathBuf,`:

```rust
    tmux: PathBuf,
    systemctl: PathBuf,
    ps: PathBuf,
```

and to `Default` after `kubectl: PathBuf::from("kubectl"),`:

```rust
            tmux: PathBuf::from("tmux"),
            systemctl: PathBuf::from("systemctl"),
            ps: PathBuf::from("ps"),
```

In `values`, replace the `let (bin, key, dir) = match provider { ... };` block with:

```rust
            let (bin, key, dir) = match provider {
                Provider::FrecentDirs => (&self.zoxide, String::new(), Path::new("/")),
                _ if status => (&self.git, cwd.to_string_lossy().into_owned(), cwd),
                Provider::TmuxSessions | Provider::TmuxWindows => (&self.tmux, String::new(), Path::new("/")),
                Provider::SystemdUnits => (&self.systemctl, String::new(), Path::new("/")),
                Provider::Processes | Provider::ProcessNames => (&self.ps, String::new(), Path::new("/")),
                _ => (&self.docker, String::new(), Path::new("/")),
            };
```

In `query`, add before `Provider::FrecentDirs => &["query", "--list"],`:

```rust
        Provider::DockerNetworks => &["network", "ls", "--format", "{{json .}}"],
        Provider::DockerVolumes => &["volume", "ls", "--format", "{{json .}}"],
        Provider::TmuxSessions => &["list-sessions", "-F", "#{session_name}\x1f#{session_windows} windows#{?session_attached, attached,}"],
        Provider::TmuxWindows => &["list-windows", "-a", "-F", "#{session_name}:#{window_index}\x1f#{window_name}"],
        Provider::SystemdUnits => &["list-units", "--all", "--plain", "--no-legend", "--no-pager", "--full"],
        Provider::Processes | Provider::ProcessNames => &["-A", "-o", "pid=,comm="],
```

- [ ] **Step 5: Parsers**

In `parse`, insert before `if provider == Provider::FrecentDirs {`:

```rust
    if matches!(provider, Provider::TmuxSessions | Provider::TmuxWindows) {
        // `name<US>about`, one per line, from tmux's own -F format.
        let mut seen = std::collections::HashSet::new();
        return text
            .lines()
            .take(5000)
            .filter_map(|l| l.split_once('\x1f'))
            .filter(|(n, _)| !n.is_empty() && n.len() <= 256 && !n.starts_with('-') && !n.chars().any(char::is_control) && seen.insert(n.to_string()))
            .map(|(n, about)| Suggestion { name: n.to_owned(), description: Some(about.chars().filter(|c| !c.is_control()).take(80).collect()), insert_value: None })
            .collect();
    }
    if provider == Provider::SystemdUnits {
        // `UNIT LOAD ACTIVE SUB DESCRIPTION...` with --plain --no-legend.
        let mut seen = std::collections::HashSet::new();
        return text
            .lines()
            .take(5000)
            .filter_map(|l| {
                let mut f = l.split_whitespace();
                let (unit, _load, active, _sub) = (f.next()?, f.next()?, f.next()?, f.next()?);
                let about = f.collect::<Vec<_>>().join(" ");
                let ok = unit.contains('.') && unit.len() <= 256 && unit.bytes().all(|b| b.is_ascii_alphanumeric() || b"@._:\\-".contains(&b)) && seen.insert(unit.to_string());
                ok.then(|| Suggestion { name: unit.to_owned(), description: Some(format!("{active} · {about}").chars().filter(|c| !c.is_control()).take(80).collect()), insert_value: None })
            })
            .collect();
    }
    if matches!(provider, Provider::Processes | Provider::ProcessNames) {
        // `  PID COMMAND`: the command is a name on Linux and a path on macOS; its last part is the name.
        let mut seen = std::collections::HashSet::new();
        return text
            .lines()
            .take(10000)
            .filter_map(|l| {
                let (pid, command) = l.trim_start().split_once(char::is_whitespace)?;
                let name = command.trim().rsplit('/').next()?;
                let ok = !pid.is_empty() && pid.bytes().all(|b| b.is_ascii_digit()) && !name.is_empty() && !name.starts_with('-') && name.len() <= 256 && !name.chars().any(char::is_control);
                if !ok {
                    return None;
                }
                if provider == Provider::Processes {
                    Some(Suggestion { name: pid.to_owned(), description: Some(name.to_owned()), insert_value: None })
                } else {
                    seen.insert(name.to_owned()).then(|| Suggestion { name: name.to_owned(), description: Some("process".to_owned()), insert_value: None })
                }
            })
            .collect();
    }
```

In the Docker JSON branch at the end of `parse`, replace the `let name = if provider == Provider::DockerImages { ... } else { row["Names"].as_str()?.to_owned() };` expression with:

```rust
            let name = match provider {
                Provider::DockerImages => {
                    let repository = row["Repository"].as_str()?;
                    let tag = row["Tag"].as_str()?;
                    if repository == "<none>" || tag == "<none>" {
                        return None;
                    }
                    format!("{repository}:{tag}")
                }
                Provider::DockerNetworks | Provider::DockerVolumes => row["Name"].as_str()?.to_owned(),
                _ => row["Names"].as_str()?.to_owned(),
            };
```

and the description key `row[if provider == Provider::DockerImages { "ID" } else { "Image" }]` with:

```rust
            let description = row[match provider {
                Provider::DockerImages => "ID",
                Provider::DockerNetworks | Provider::DockerVolumes => "Driver",
                _ => "Image",
            }]
```

- [ ] **Step 6: Run the tests**

Run: `cargo test -p goh-daemon --lib providers::tests && cargo test -p goh-core`
Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add crates/goh-core/src/spec.rs docs/schemas/completion.schema.json crates/goh-daemon/src/providers.rs
git commit -m "feat(daemon): providers for docker networks and volumes, tmux, systemd units and processes"
```

---

### Task 2: Point spec arguments at the providers

**Files:**
- Modify: `tools/import-specs/enhance.mjs`
- Modify: `crates/goh-core/specs/{docker,tmux,systemctl,kill,pkill,killall}.json`
- Test: `crates/goh-core/src/bundled.rs`

**Interfaces:**
- Consumes: provider names from Task 1.
- Produces: `docker network {rm,inspect,connect,disconnect}` `NETWORK` and `docker volume {rm,inspect}` `VOLUME` arguments → `docker-networks` / `docker-volumes`; tmux `-t` of session commands → `tmux-sessions`, of window commands → `tmux-windows`; every systemctl argument named like `UNIT` → `systemd-units`; `kill` arguments → `processes`; `pkill` and `killall` arguments → `process-names`.

- [ ] **Step 1: Write the failing test**

In `crates/goh-core/src/bundled.rs`, inside `mod tests` (after `wrappers_and_assignments_hand_the_rest_of_the_line_to_the_command_they_run`):

```rust
    #[test]
    fn sessions_units_networks_and_processes_reach_their_arguments() {
        let cwd = tempfile::tempdir().unwrap();
        let engine = Engine::new(bundled_specs());
        let shell = ShellContext::default();
        let values = |p: Provider, _: &std::path::Path| {
            let name = match p {
                Provider::TmuxSessions => "work",
                Provider::TmuxWindows => "work:1",
                Provider::SystemdUnits => "nginx.service",
                Provider::DockerNetworks => "backend",
                Provider::DockerVolumes => "pgdata",
                Provider::Processes => "4242",
                Provider::ProcessNames => "node",
                _ => return vec![],
            };
            vec![Suggestion { name: name.into(), description: None, insert_value: None }]
        };
        for (line, label) in [("tmux attach-session -t ", "work"), ("tmux kill-session -t ", "work"), ("tmux select-window -t ", "work:1"),
            ("systemctl restart ", "nginx.service"), ("docker network rm ", "backend"), ("docker volume rm ", "pgdata"),
            ("kill ", "4242"), ("pkill ", "node"), ("killall ", "node")] {
            let (found, _) = labels(&engine, line, cwd.path(), &values, &shell);
            assert!(found.contains(&label.to_string()), "{line:?}: {found:?}");
        }
        let (found, _) = labels(&engine, "docker network create ", cwd.path(), &values, &shell);
        assert!(!found.contains(&"backend".to_string()), "a new network has no name yet: {found:?}");
    }
```

- [ ] **Step 2: Run it to see it fail**

Run: `cargo test -p goh-core --lib bundled::tests::sessions_units_networks_and_processes_reach_their_arguments`
Expected: FAIL on `"tmux attach-session -t "` (no provider on that argument).

- [ ] **Step 3: Rules in the importer**

In `tools/import-specs/enhance.mjs`, before `const only = (arg, extensions) => ...`, add:

```js
  // Live values from goh's fixed read-only providers (crates/goh-daemon/src/providers.rs); nothing here runs.
  const argsOf = node => node?.args ?? [];
  if (spec.name === 'docker') {
    const sub = (group, name) => spec.subcommands?.find(s => s.name === group)?.subcommands?.find(s => s.name === name);
    for (const name of ['rm', 'inspect', 'connect', 'disconnect']) for (const arg of argsOf(sub('network', name))) if (arg.name === 'NETWORK') arg.provider = 'docker-networks';
    for (const name of ['rm', 'inspect']) for (const arg of argsOf(sub('volume', name))) if (arg.name === 'VOLUME') arg.provider = 'docker-volumes';
    changes.push({source:'https://docs.docker.com/reference/cli/docker/network/', feature:'network-and-volume-names'});
  }
  if (spec.name === 'tmux') {
    const sessions = ['a', 'attach', 'attach-session', 'has-session', 'kill-session', 'lock-session', 'rename-session', 'switch-client'];
    const windows = ['select-window', 'kill-window', 'rename-window', 'swap-window', 'move-window', 'link-window', 'unlink-window', 'respawn-window'];
    for (const node of spec.subcommands ?? []) {
      const provider = sessions.includes(node.name) ? 'tmux-sessions' : windows.includes(node.name) ? 'tmux-windows' : null;
      if (provider) for (const option of node.options ?? []) if (option.name === '-t') withProvider(option, provider);
    }
    changes.push({source:'https://man.openbsd.org/tmux.1#COMMANDS', feature:'session-and-window-targets'});
  }
  if (spec.name === 'systemctl') {
    walk(spec, node => { for (const arg of node.args ?? []) if (/UNIT/.test(arg.name ?? '')) arg.provider = 'systemd-units'; });
    changes.push({source:'https://www.freedesktop.org/software/systemd/man/latest/systemctl.html', feature:'unit-names'});
  }
  if (spec.name === 'kill') for (const arg of spec.args ?? []) arg.provider = 'processes';
  if (spec.name === 'pkill' || spec.name === 'killall') for (const arg of spec.args ?? []) arg.provider = 'process-names';
  if (['kill', 'pkill', 'killall'].includes(spec.name)) changes.push({source:'https://pubs.opengroup.org/onlinepubs/9699919799/utilities/kill.html', feature:'process-arguments'});
```

- [ ] **Step 4: Apply the rules to the committed specs**

`docker.json` carries hand edits, so do not regenerate it. Apply only the `enhance` function to the six files, in place:

```bash
node --input-type=module -e "
import fs from 'node:fs';
import { enhance } from './tools/import-specs/enhance.mjs';
for (const name of ['docker', 'tmux', 'systemctl', 'kill', 'pkill', 'killall']) {
  const path = 'crates/goh-core/specs/' + name + '.json';
  const spec = JSON.parse(fs.readFileSync(path, 'utf8'));
  enhance(spec);
  fs.writeFileSync(path, JSON.stringify(spec, null, 2) + '\n');
}"
git diff --stat crates/goh-core/specs
```

Expected: only those six files changed. `git diff crates/goh-core/specs/docker.json` must show only `"provider"` lines added and, at most, global options the docker rules re-add (`addOptions` skips existing ones, so normally none). If anything else changed, `git checkout -- crates/goh-core/specs/docker.json` and add the `provider` fields to the `NETWORK`/`VOLUME` arguments by hand instead.

- [ ] **Step 5: Run the tests**

Run: `cargo test -p goh-core && (cd tools/import-specs && npm test)`
Expected: PASS. `npm run check` is not run: it already fails on the hand-edited specs (see `tools/import-specs/README.md`).

- [ ] **Step 6: Commit**

```bash
git add tools/import-specs/enhance.mjs crates/goh-core/specs crates/goh-core/src/bundled.rs
git commit -m "feat(spec): suggest networks, volumes, tmux targets, units and processes"
```

---

### Task 3: Feature spec

**Files:**
- Modify: `docs/features/project-context.md`

- [ ] **Step 1: Update the spec**

In the bullet `- \`$CDPATH\` folders; kubectl pods and namespaces, Docker containers and images.` change the end to:

```markdown
- `$CDPATH` folders; kubectl pods and namespaces; Docker containers, images, networks and volumes; tmux sessions
  and windows (`tmux attach-session -t`); systemd units (`systemctl restart`); running processes for `kill` (by pid)
  and `pkill`/`killall` (by name).
```

Set `updated:` to today and add under `## Changes`:

```markdown
- unreleased: Docker networks and volumes, tmux sessions and windows, systemd units and running processes are suggested.
```

- [ ] **Step 2: Check and commit**

Run: `scripts/check-feature-specs.sh`
Expected: exit 0.

```bash
git add docs/features/project-context.md
git commit -m "docs: list the new live values"
```
