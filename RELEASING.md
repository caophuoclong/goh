# Releasing

A release is a merge. Nothing is run by hand after the one-time setup.

```text
push to main ──► release PR opened/updated (version + CHANGELOG, from the commits)
                         │ you merge it
                         ▼
        tag vX.Y.Z + GitHub release ──► build 4 packages ──► attach them + SHA256SUMS ──► update the Homebrew tap
```

Packages (`goh-X.Y.Z-<target>.tar.gz`, with one `SHA256SUMS` file for all): `aarch64-apple-darwin`, `x86_64-apple-darwin`,
`aarch64-unknown-linux-gnu`, `x86_64-unknown-linux-gnu`. All contain `goh`, `gohd`, `goh-popup`; on macOS `bin/goh-popup`
links to `libexec/Goh.app`. Linux uses the verified X11/AT-SPI popup path or inline fallback;
native Wayland floating surfaces are not implemented. See [Linux popup backends](docs/linux-popup.md).
Linux popup runtime libraries include libX11, libxkbcommon-x11, EGL/OpenGL and the desktop accessibility D-Bus service. `install.sh` and the Homebrew formula use exactly these files.

## One-time setup

1. **Repository settings → Actions → General**: set *Workflow permissions* to "Read and write" and tick
   "Allow GitHub Actions to create and approve pull requests" (release-please needs both).
2. **First release (1.0.0).** The workspace is already at 1.0.0 and `.release-please-manifest.json` says so; push the
   code and the tag together, so the release PR is computed from 1.0.0 on:

   ```bash
   git tag v1.0.0
   git push origin main v1.0.0
   ```

   The tag starts `release.yml`, which builds the packages and creates the GitHub release.
3. **Homebrew tap.** Create an empty public repository `caophuoclong/homebrew-goh` (the `homebrew-` prefix is
   required), then a fine-grained token with *Contents: read and write* on that repository only, and save it in this
   repository as the secret `HOMEBREW_TAP_TOKEN`. Without it the release still works and only the tap is skipped (the
   run summary says so). To use another tap, set the repository variable `HOMEBREW_TAP_REPO`.
   Users then run `brew install caophuoclong/goh/goh`.
4. **macOS signing certificate.** Run `scripts/macos-signing-cert.sh <safe folder>` once and store the two files it
   writes as the secrets `MACOS_SIGN_P12` and `MACOS_SIGN_P12_PASSWORD` (it prints the `gh secret set` commands). The
   macOS packages sign `Goh.app` (the bundle around goh-popup) with it, so macOS keeps its own Accessibility permission
   (`popup.ownPermission`) through updates. Keep the files: a new certificate makes every user allow Goh again.
   Without the secrets the bundle is signed ad hoc and the run summary says so.

## Every release

Merge the open "chore(main): release X.Y.Z" PR. To rebuild a release, run *Build and publish packages* by hand with
its tag (it re-uploads over the existing assets).

## Trying the packaging locally

```bash
scripts/package.sh host v1.0.0                    # dist/goh-1.0.0-<your target>.tar.gz and .sha256
scripts/render-formula.sh v1.0.0 SHA256SUMS       # the Homebrew formula, from the four checksums
```

`GOH_RELEASES=file:///some/dir ./install.sh --version 1.0.0` installs from a folder laid out like a release
(`download/v1.0.0/<files>`) instead of GitHub.

## If something fails

- The release PR has a stale `Cargo.lock`: the `lockfile` job fixes it on the PR branch. A push made by that job does
  not re-run the PR's checks; close and reopen the PR if you need them.
- A build job fails: fix on `main`, then re-run *Build and publish packages* with the tag (the tag keeps pointing at the
  release commit; fix forward with a `fix:` commit and let the next release carry it if the problem is in the code).
