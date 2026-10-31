# Static completion import

This developer tool converts selected Fig declarations to goh's JSON schema. Runtime goh remains Rust-only,
offline, and independent of Fig/Amazon Q, Node, npm and this importer. Cargo embeds every checked-in JSON
file in `crates/goh-core/specs`; user specs still replace the bundled command of the same name.

## Reproduce

Requires Node 24+ for the importer and its development dependency, TypeScript 5.9.3. A normal `cargo build`
does **not** need Node, the upstream checkout, or network access.

```sh
cd tools/import-specs
npm ci --ignore-scripts
npm test
npm run import
npm run check
```

`sources.json` pins the repository, commit, command selection and explicitly selected static `loadSpec`
references. Downloads are cached under the ignored `target/spec-import/<commit>/` directory. With that cache,
regeneration and `--check` work offline. On Node installations using an HTTP proxy, enable Node's environment
proxy support with `NODE_USE_ENV_PROXY=1`. The tool retries transient download failures, and completes all
conversions before writing generated files. It retains the hand-written git/npm specs and now generates the expanded Docker spec.

## What is converted

The TypeScript compiler API parses source as syntax; source modules, top-level statements, generators,
arbitrary function calls and shell commands are never executed. The converter reads object/array literals,
constants, aliases, local imports, static property access and spreads. Object-bodied root factories are
inspected as literals; their defaults and bodies are not invoked. Four pinned Docker generator references are projected to three fixed native read-only providers; their
upstream scripts and postprocessors are never executed. `isCommand` retains the delegated command boundary.
Known `filepaths(...)` declarations are
projected onto goh's own filesystem enumeration, including folder-only arguments.

Aliases become separate static names. Persistent options are copied to descendants. Argument declarations,
literal suggestions and the supported file/folder templates are retained. Imported command/option names
are restricted to safe token characters; suggestion names use goh's existing shell escaping. Custom insertion
snippets and cursor placeholders are omitted. All output specs are validated against the JSON schema by the
Rust tests, and are exercised through engine, daemon and shell tests.

## Coverage and limitations

This pack generates 170 command specs (the first 36, 39 common tools such as `jq`, `tmux`, `systemctl`, `psql`, `tsc`, `vim`, and 18 asked for in Fig's issue tracker: `gradle`, `mvn`, `dotnet`, `bundle`, `gem`, `rake`, `ruby`, `java`, `flutter`, `swift`, `minikube`, `k9s`, `rclone`, `ffmpeg`, `launchctl`, `defaults`, `serverless`, `pipenv`), plus 77 more; with the hand-written git, npm, just, task and deno specs and 15 small wrapper specs (`sudo`, `time`, `env`, `xargs`, `man`, `which`, ...), 191 commands are bundled. The 39 common tools and the 18 newest are exactly what `npm run import` writes, except `pipenv run`, which was pointed at the `python-scripts` provider by hand. A further batch adds common developer tools (process tools, Python/JS/Rust/C toolchains, cloud CLIs, mobile tooling). The `cmake` spec covers configure options but not `cmake --build` (absent upstream), and names may now contain `_` (`pg_dump`). `rails`, `composer`, `php` and `deno` build their specs at runtime upstream and cannot be imported; `deno` is hand-written. `az` and `wp` were left out for their size. Fig's `isRepeatable` is kept. Several of the first 36 (`cd`, `helm`, `docker`, `npx`, `poetry`, `uv`, `kubectl`, `yarn`, `pnpm`, `bun`, ...) were edited after import, so `npm run check` fails on them until those edits move into `enhance.mjs`; do not run `npm run import` over them without reviewing `git diff` first. It is static coverage, not full Fig runtime compatibility.
Some upstream files describe particular CLI versions or macOS/BSD flags; validate flags against your installed
tool when needed. This is a snapshot pinned to the recorded commit, not a live update service.

Dynamic generators, project scripts/branches, remote package search, context-dependent spec functions, parser
directives, mutually exclusive/dependent flags, custom snippets and unsupported syntax are omitted and recorded
in `report.json`. Optional/variadic metadata does not add parsing capabilities that goh's engine does not have.
File extension filters are not implemented by goh's filesystem templates. The cloud specs include their service
groups plus selected nested trees: AWS S3/configure/STS, gcloud auth/config/run, and Docker Compose. Other `loadSpec` references
remain listed as omissions; they do not execute or fetch anything at runtime.

`report.json` records each source file's SHA-256, upstream revision, command counts and omissions. It is generated
deterministically alongside the JSON. Inspect it when adding or updating a command. Do not count a source file
as fully supported just because its top-level name appears in the catalog.

## Attribution

Source: <https://github.com/withfig/autocomplete>, commit recorded in `sources.json`.

Copyright (c) 2021 Hercules Labs Inc. (Fig). The imported declarations are distributed under MIT; the complete
upstream license is preserved in [`../../third_party/fig-autocomplete/LICENSE`](../../third_party/fig-autocomplete/LICENSE).
The JSON files are derived data; the build does not embed or execute upstream TypeScript.

`enhance.mjs` records a small set of reviewed additions from official Docker/Go documentation: global
Docker context selectors, missing Go package argument slots, and common Go test flags. The generated report
lists these additions with their source URLs. They remain declarative data.

## Reviewed additions (`enhance.mjs`)

Data-only additions applied after conversion: Docker's global selectors; Go's package arguments and test flags;
manifest-backed providers for `cargo` (`--bin`, `--example`, `-p`, `--features`), `pnpm --filter` and
`docker compose` services (`compose-services`, and `-f` only before the subcommand); file-type filters for `python`,
`node`, `unzip` and `tar x/t -f`. The hand-written `git`, `npm`, `just` and `task` specs carry their own providers.
