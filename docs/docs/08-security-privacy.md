# 08 — Security, Privacy & Threat Model

## Default privacy posture

- All static autocomplete works locally and offline; **no account**; **no network required**.
- The app may receive content typed at an interactive prompt. This is **potentially sensitive data** (tokens, paths, credentials, command arguments), even when no command executes. Treat it as ephemeral confidential input.
- No raw command buffer, command history or environment-variable values in persistent logs, crash dumps, support reports or analytics by default.
- User controls whether dynamic providers run; any external registry/update requires separate opt-in and transparent network description.

## Assets, trust boundaries

Assets: editable line buffer, cwd, local files, terminal focus state, per-user IPC, completion specs, launch agent/startup profiles, release artifacts. Trust boundaries: shell adapter → daemon; daemon → provider subprocess; daemon → UI; installer → shell rc/profile; optional downloaded specs → importer.

## STRIDE-style threat analysis

| Threat | Scenario | Mitigation |
|---|---|---|
| Spoofing | Another local user sends fake IPC events | Unix socket permissions + peer creds / Windows named pipe ACL + handshake |
| Tampering | Malicious local spec inserts unexpected edit text | JSON schema validation, safe range checks, no spec code execution |
| Repudiation | Changes to shell rc aren't obvious | Preview diff, bounded marker block, backup and uninstall log metadata |
| Information disclosure | Secret pasted in command line appears in diagnostic dump | No raw buffers in logs; redact generated diagnostics; opt-in support bundle review |
| Denial of service | Huge paste, slow provider, malformed socket message | Message size limit, debounce, deadline, bounded queue, cancellations |
| Elevation | Generator invokes shell or reads files outside policy | No `sh -c`, allowlisted argv, sandbox where OS permits, user consent, no privileged mode |
| Supply chain | Compromised spec/installer | Pinned dependencies, SBOM, checksum/signatures, verified release process |

## Generator policy

**Level 0 (default):** static JSON specs + filesystem enumeration under current user privileges. File paths are read for names only; do not open contents for completion.

**Level 1 (explicit enable):** shipped read-only Rust providers (`git branches`, e.g.) with bounded commands and narrowly allowed working directory. User may disable by provider. Fail closed when policy cannot be enforced.

**Level 2 (future):** third-party script generators, isolated process/container/WASM if feasible, permission manifest, per-provider consent, no ambient network or broad filesystem privileges. Must **not** be shipped as silent defaults.

## Minimum controls

1. **No global keylogging:** shell-scoped only while editable prompt; do not intercept secure inputs or other apps.
2. **Local IPC:** chmod/ACL; OS peer identity where supported; validate all input length/schema/version; no world-readable files.
3. **Secrets:** never include buffer in exceptions, metrics, traces, panic messages, crash reports; disable core dumps or implement redaction where practicable.
4. **Insertion:** adapter validates snapshot hash + generation and UTF-8 range; never accept edit from older state.
5. **Filesystem:** do not follow symlink into disallowed paths if a future restricted provider policy requires it; default enumeration runs with user privileges and no data upload.
6. **Permissions:** show OS Accessibility permission only if necessary for requested display mode; don't insist if inline mode works.
7. **Updates:** download only after user opt-in or explicit command (MVP manual); HTTPS + signature verification + rollback.
8. **Uninstall:** remove managed rc/profile lines, service, socket/config if requested; never remove unrelated user data.

## Privacy settings example

```json
{
  "privacy": {
    "telemetry": false,
    "networkSpecs": false,
    "dynamicGenerators": false,
    "retainCommandHistory": false
  }
}
```

## Security release gate

- IPC unauthorized user test pass; fuzz parser/protocol/spec loader; no command text in structured logs; no network requests in offline smoke test; generator timeout/cancel tests; reproducible or verifiable build provenance.
- Audit shell snippets manually for quoting, arbitrary expansion and user rc modification; test privilege boundaries on every OS.
- Responsible disclosure policy and security contacts added before public launch (address TBD).
