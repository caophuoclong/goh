# More Bundled Specs Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Bundle about 85 more commands (from 114 to about 200) so that common tools people type every day (`kill`, `lsof`, `pytest`, `ruff`, `podman`, `pulumi`, `cmake`, `adb`, `nx`, `bat`, `fd`, ...) complete out of the box.

**Architecture:** No new code. The pinned importer in `tools/import-specs` already converts Fig's TypeScript declarations into goh's static JSON, never running any of it. This plan adds names to its manifest, regenerates, keeps the hand-edited specs untouched, and extends the engine test that proves imported commands complete. Long-tail commands stay covered by `goh spec gen` / `completion.autoSpecs`, so a bulk import of all 700 Fig specs is not part of this plan.

**Tech Stack:** Node 24+ with TypeScript 5.9.3 (importer only, never at runtime), Rust tests in `crates/goh-core`.

**Spec:** `docs/features/bundled-specs.md`, `tools/import-specs/README.md`.

## Global Constraints

- Every user-visible change updates its spec in `docs/features/` in the same commit series: text, `updated: <today>`, and a `- unreleased: ...` line under `## Changes`. Run `scripts/check-feature-specs.sh` before the last commit.
- Conventional Commits; end every commit message with `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.
- The upstream revision stays pinned at `aef52acff84c45edde61ae610cc2c964802b9a38`; do not bump it in this plan.
- Never run `npm run import` without reviewing `git diff` afterwards: specs edited after import (`cd`, `helm`, `docker`, `npx`, `poetry`, `uv`, `kubectl`, `yarn`, `pnpm`, `bun`, ...) must keep their edits.
- Specs carry no scripts; generators in Fig sources are dropped and listed in `report.json`.
- A single spec over 1 MB of JSON is left out (as `az` was) and named in `tools/import-specs/README.md`.

---

## File Structure

| File | Change |
|---|---|
| `tools/import-specs/sources.json` | New names in `commands` |
| `crates/goh-core/specs/*.json` | New generated files only |
| `tools/import-specs/report.json` | Regenerated |
| `crates/goh-core/src/bundled.rs` | More cases in `imported_commands_work_through_the_engine` |
| `README.md`, `docs/features/bundled-specs.md`, `tools/import-specs/README.md` | New counts |

---

### Task 1: Import the new commands

**Files:**
- Modify: `tools/import-specs/sources.json`
- Create: `crates/goh-core/specs/<name>.json` for each new command
- Modify: `tools/import-specs/report.json`
- Test: `crates/goh-core/src/bundled.rs`

**Interfaces:**
- Produces: one JSON spec per name below in `crates/goh-core/specs/`. Plan `2026-10-31-more-providers.md` later points `kill`, `pkill` and `killall` arguments at new providers, so it runs after this task.

- [ ] **Step 1: Write the failing test**

In `crates/goh-core/src/bundled.rs`, extend the table in `imported_commands_work_through_the_engine`:

```rust
        for (line, label) in [("pnpm a", "add"), ("bun r", "run"), ("brew in", "install"),
            ("uv sy", "sync"), ("cargo bu", "build"), ("kubectl ge", "get"),
            ("terraform pl", "plan"), ("gh pr cr", "create"), ("aws s", "s3"),
            ("aws s3 c", "cp"), ("gcloud run dep", "deploy"),
            ("gcloud co", "compute"), ("curl --con", "--connect-timeout"), ("ssh -", "-p"),
            ("kill -", "-9"), ("lsof -", "-i"), ("ollama r", "run"), ("pytest --co", "--collect-only"),
            ("cmake --bu", "--build"), ("pod ins", "install"), ("adb dev", "devices"),
            ("podman p", "ps"), ("pulumi u", "up"), ("conda ins", "install")] {
```

- [ ] **Step 2: Run it to see it fail**

Run: `cargo test -p goh-core --lib bundled::tests::imported_commands_work_through_the_engine`
Expected: FAIL on `"kill -"` (no `kill` spec yet).

- [ ] **Step 3: Add the names to the manifest**

In `tools/import-specs/sources.json`, append to `"commands"` (after `"java"`), keeping the list order:

```json
    "kill",
    "killall",
    "pkill",
    "lsof",
    "open",
    "ollama",
    "kind",
    "k3d",
    "podman",
    "pulumi",
    "firebase",
    "wrangler",
    "netlify",
    "flyctl",
    "doctl",
    "kubectx",
    "kubens",
    "pyenv",
    "nvm",
    "rbenv",
    "asdf",
    "volta",
    "fnm",
    "conda",
    "ruff",
    "black",
    "pytest",
    "mypy",
    "rustc",
    "clang",
    "gcc",
    "cmake",
    "bazel",
    "zig",
    "xcodebuild",
    "xcrun",
    "pod",
    "fastlane",
    "adb",
    "ng",
    "nest",
    "nx",
    "lerna",
    "expo",
    "eas",
    "playwright",
    "jest",
    "http",
    "mongosh",
    "mysql",
    "pg_dump",
    "nc",
    "nmap",
    "dig",
    "ping",
    "pandoc",
    "nginx",
    "vault",
    "ansible-playbook",
    "terragrunt",
    "helmfile",
    "sam",
    "cdk",
    "amplify",
    "hugo",
    "mix",
    "stack",
    "exa",
    "eza",
    "bat",
    "fd",
    "zoxide",
    "htop",
    "btop",
    "watchman",
    "direnv",
    "gum"
```

`pg_dump` has an underscore: the manifest check in `import.mjs:13` only allows `^[a-z][a-z0-9-]*$`. Change that pattern to `^[a-z][a-z0-9_-]*$` in the same edit.

- [ ] **Step 4: Run the importer**

```bash
cd tools/import-specs && npm ci --ignore-scripts && npm test && npm run import; cd ../..
```

Expected: exits 0 and prints the per-command counts. If it throws `Empty imported spec: <name>` or a conversion error for a name, remove that name from `sources.json` and run `npm run import` again; note the name for Task 2.

- [ ] **Step 5: Keep the hand-edited specs as they were**

```bash
git status --porcelain crates/goh-core/specs | awk '$1 == "M" {print $2}' | xargs -r git checkout --
git status --porcelain crates/goh-core/specs
```

Expected: only `??` lines (new files) remain.

- [ ] **Step 6: Leave out oversized specs**

```bash
find crates/goh-core/specs -name '*.json' -size +1024k -newer tools/import-specs/sources.json
```

Expected: nothing. For any file listed, delete it, remove its name from `sources.json`, re-run Step 4 and Step 5, and note the name for Task 2.

- [ ] **Step 7: Run the tests**

Run: `cargo test -p goh-core`
Expected: PASS, including `every_bundled_spec_parses_and_covers_its_command` (it fails on a spec that does not parse or is empty) and the new table rows. If a row fails only because the label differs (for example the spec offers `-KILL` rather than `-9`), open the generated JSON, pick the label it really offers for that prefix, and update the row; do not edit the generated JSON.

- [ ] **Step 8: Run the whole suite**

Run: `cargo test --workspace --locked`
Expected: PASS (`crates/goh-core/tests/golden.rs` validates the specs against `docs/schemas/completion.schema.json`).

- [ ] **Step 9: Commit**

```bash
git add tools/import-specs/sources.json tools/import-specs/import.mjs tools/import-specs/report.json crates/goh-core/specs crates/goh-core/src/bundled.rs
git commit -m "feat(spec): bundle about 85 more commands"
```

---

### Task 2: Counts and docs

**Files:**
- Modify: `README.md:11`, `docs/features/bundled-specs.md`, `tools/import-specs/README.md:44`

- [ ] **Step 1: Count**

```bash
ls crates/goh-core/specs/*.json | wc -l
```

Use that number as `N` below.

- [ ] **Step 2: Update the docs**

- `README.md` line 11: `114 bundled` becomes `N bundled`, and add `pytest`, `ruff`, `podman`, `lsof` to the example list.
- `docs/features/bundled-specs.md`: `for 114 common commands` becomes `for N common commands`; set `updated:` to today; first line under `## Changes`: `- unreleased: about 85 more specs (kill, lsof, pytest, ruff, podman, pulumi, cmake, adb, nx, bat, fd, ...).`
- `tools/import-specs/README.md` line 44: replace the first sentence's counts with the new totals and add, after the sentence about the 18 newest: `A further batch adds common developer tools (process tools, Python/JS/Rust/C toolchains, cloud CLIs, mobile tooling).` If Task 1 left any name out, add `<name> was left out: <reason>.`

- [ ] **Step 3: Check and commit**

Run: `scripts/check-feature-specs.sh`
Expected: exit 0.

```bash
git add README.md docs/features/bundled-specs.md tools/import-specs/README.md
git commit -m "docs: count the new bundled specs"
```
