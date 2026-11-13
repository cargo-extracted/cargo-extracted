# Contributing

The crate is generated. To change it, change the patch series. To move to a
newer Cargo, run a sync. Changes to what Cargo's code does are out of scope
here and go to rust-lang/cargo; the patches only move code, delete what
cannot exist outside Cargo, make things `pub`, and add glue in new files.

## Layout

| Path | What it is | Edit? |
|---|---|---|
| `patches/` | The extraction, as a `git am` series on top of rust-lang/cargo | yes, the source of truth |
| `patches/upstream/` | Refactors of Cargo that keep it building and are candidates for upstream pull requests; exported from commits with the `Cargo-Upstream: candidate` trailer and applied first | yes |
| `sync/` | The sync tool and `sync/config.toml` | yes |
| `.github/workflows/` | CI, the sync, publishing | yes |
| `crates/conformance/` | Differential tests against the pinned Cargo binary | yes |
| `crates/cargo-resolver/`, `crates/resolver-tests/` | Generated from upstream plus patches | no |
| `Cargo.toml`, `Cargo.lock`, `upstream.toml` | Generated | no |

## The patch series

A short series per crate, each patch one body of work a reviewer can follow
on its own. Only the complete series compiles. The resolver's:

1. Move the files into `crates/<name>/src/` at the paths they have in
   Cargo's `src/`, with the crate manifest, `lib.rs` and the `mod.rs` files
   that declare the same module tree and re-export what Cargo's do.
2. Copy the small pieces the moved code reads from Cargo's larger files,
   verbatim with the Cargo-only parts left out, each listed under `[drift]`.
3. Stand in for `GlobalContext` and `Shell`, with Cargo's method names, over
   what the embedding application supplies.
4. Stand in for the other Cargo types the moved code holds (`Workspace`,
   `Package`, `Target`, `RustcTargetData`).
5. Drop what cannot exist outside Cargo: the `#[cfg(any())]` marks and `pub`
   changes inside upstream files. This is the patch to read closely.

The rule for upstream files holds across the series: moved as they are, and
then only `#[cfg(any())]` on items that cannot exist outside Cargo, and
visibility. The `cfg` removes an item from the build like a deletion would,
but the hunk touches only the item's header, so upstream edits inside the
item merge through; the same series applies to the current nightly and to
the release branches behind it. No function is added to an upstream file and
no function changes behavior; anything beyond that is a new file, and names
the moved code imports but no longer calls are empty stand-ins so the import
lines stay. The resolver's series touches 9 hunks of upstream code, 17 lines
in all. A conflict is then an upstream edit to an item header or a `use`
line next to one of ours, in the last patch, and the fix follows upstream.

Pieces copied by hand rather than moved, because only a fraction of the file
was wanted, are listed under `[drift]` in `sync/config.toml`; every sync
reports upstream commits that touched them for a human to review. Keep that
table current when you copy something.

## Syncing

Requires Python 3.11+, git, and a Rust toolchain at least as new as the
crates' `rust-version`, which is Cargo's own; `cargo +beta` works between
releases.

```sh
python3 sync/sync.py sync nightly --lag 1    # yesterday's nightly, what the Sync workflow does
python3 sync/sync.py sync stable             # the current stable release
python3 sync/sync.py sync 1.99.1             # a specific release
python3 sync/sync.py sync 3960fa69b          # a specific Cargo commit
python3 sync/sync.py plan                    # what the Sync workflow would do, per branch
python3 sync/sync.py verify                  # the tree is exactly upstream plus patches
```

`--repo ~/Code/cargo` (or `CARGO_EXTRACTED_UPSTREAM`) uses a local Cargo
checkout. `sync` exits 3 when already at that commit (`--force` regenerates),
2 on a conflict. `verify` applies the series without a three-way merge; a
conflict there means the series is not rebased onto the pinned commit, and
`sync <commit> --force` re-exports it. `verify --no-lock` skips `Cargo.lock`
and needs no network.

When the patches no longer apply, the sync stops in the middle of `git am` in
`.sync/work`:

```sh
python3 sync/sync.py sync nightly          # exits 2, lists the conflicting files
cd .sync/work
# resolve, then:
git add -A && git am --continue
cd -
python3 sync/sync.py export-patches
python3 sync/sync.py sync nightly
```

The sync report (`--report FILE`, or the pull request body in CI) lists the
upstream commits that touched the files under `[drift]`. Those are copies, so
git merged nothing for them; read each commit and port what applies by
amending the copy's commit in `.sync/work`, then export and sync again.
Commit `patches/` together with the regenerated tree. In CI a conflict opens
an issue labelled `sync-conflict` with these steps.

It has to be `git am -3`, not `git apply`: `am` keeps the series as commits,
which `export-patches` reads back with `git format-patch`, and `-3` is what
lets upstream edits to moved files merge through the move. Done entirely by
hand in a checkout of your own, `git am -3 patches/*.patch` at the target
commit and `git format-patch -M --zero-commit --no-signature -o patches/
<commit>..HEAD` produce the same files; the tool only adds resolving the
target and regenerating the tree.

To change the crate, do the same without a conflict: `sync <target>
--keep-work`, amend the right commit in `.sync/work` (a fixup commit and
`git rebase --autosquash` work), `export-patches`, re-sync. The kept worktree
has the crate's name added to its root manifest, so
`cargo test -p cargo-ext-resolver` works from there. Keep to the rules above.

## Adding a crate

`docs/roadmap.md` says which crates and in what order. The version is shared
and publishing is per crate, so a crate can be added to `main` and to an
existing release branch alike.

On `main`: a pull request that adds the crate's series to `patches/`, the
`[[publish]]`, `[export]` and `[drift]` entries in `sync/config.toml`, the
crate's test, package and doc lines in the workflows, and the docs. Nightly syncs carry it from then on, and every release branch
cut afterwards includes it. Inside the series, the new crate's patches go
after the crates it depends on.

On an existing release branch, say `rust-1.101.0` after `cargo-ext-resolver
0.103.0` shipped: check the branch out, run `sync <its pinned commit> --force
--keep-work`, apply the crate's patches from `main` onto `.sync/work` with
`git am -3`, resolve what the older Cargo needs, `export-patches`,
re-sync, and open a pull request into the branch with the same config and
workflow changes. Nothing bumps, because the pinned commit did not change.
Merging it runs the publish workflow, which finds the new crate missing from
crates.io at that version and publishes only it. A crate added late is only
as old as the branch it is added to.

## Testing

```sh
cargo test -p cargo-ext-resolver -p resolver-tests
pip install pre-commit && pre-commit run --all-files   # rustfmt, clippy, ruff, mypy, taplo
pre-commit install                                      # and before each commit from now on
```

`rustfmt.toml` and `clippy.toml` are Cargo's, copied in by the sync, so the
new files in the patches are held to the same formatting and lints as the
moved ones.

`resolver-tests` is Cargo's own suite and proves the algorithm is Cargo's.
`crates/conformance` proves the answers are: it resolves the same workspaces
with the Cargo binary from the pinned commit and with `cargo_resolver`, over
a local sparse index, and compares the lock files structurally. The oracle
has to be the pinned Cargo; the harness looks for `$CONFORMANCE_TOOLCHAIN`,
then the dated channel from `upstream.toml`, then `nightly`, `beta`, `stable`.

```sh
rustup toolchain install nightly-$(python3 -c 'import tomllib; print(tomllib.load(open("upstream.toml","rb"))["channel_date"])') --profile minimal
cargo test -p conformance
CONFORMANCE_KEEP=1 cargo test -p conformance -- yanked --nocapture   # keep the temp dir
```

A conformance mismatch means the extraction changed behavior, usually a
conflict resolution or an unported `[drift]` change. Fix the patch.

## Automation

The Sync workflow runs daily at 06:00 UTC and whenever it is started from
the Actions tab, for example the day a Rust release lands. Unattended, it
keeps `main` at yesterday's nightly, cuts and syncs release branches, and
opens a `sync-conflict` issue when the series stops applying. Nothing
reaches crates.io until `PUBLISH_ENABLED` is `true` and a reviewer approves
the `crates-io` environment.

| Branch | Fed by |
|---|---|
| `main` | a sync to `nightly-<yesterday>`, auto-merged when CI passes |
| `rust-1.XX.0` | cut from `main`'s tip when a Rust release first resolves onto Cargo's branch of that name, with `patches/` taken from the `main` commit whose pin precedes Cargo's branch point (`sync.py branch-point`), then synced to the release as a pull request merged by a human; point releases arrive the same way |
| `sync/<branch>` | the pending sync, force-pushed; one open pull request each |

Nightly is resolved through the dated manifests under `dist/YYYY-MM-DD/`, a
day behind (`SYNC_LAG_DAYS`), so a sync can be replayed and the commit in
`upstream.toml` is one a published toolchain was tested with; stable through
the manifest of its exact version. Tooling fixes on `main` are not carried to
release branches automatically; cherry-pick them, as Cargo backports.

- `ci.yml`: on pushes to `main` and `rust-*` and on pull requests.
  `verify`, build and test on three platforms, conformance on the pinned
  toolchain, docs with warnings denied, `cargo package` for release
  versions.
- `sync.yml`: daily, and by hand. `sync.py plan` decides which branch each
  channel lands on (`channels` also takes a release such as `1.99.1`, for a
  branch behind the current stable); `_sync-channel.yml` runs once per entry and opens or
  updates the pull request, or a `sync-conflict` issue.
- `publish.yml`: when a `rust-*` branch changes with a stable version.
  For each crate whose version is not on crates.io: checks the sibling
  crates are there, dry-runs, waits for the environment approval,
  publishes, tags `<crate>-v<version>`, creates a GitHub release.

Pull requests opened with `GITHUB_TOKEN` never trigger CI, so auto-merge
needs a bot identity. A GitHub App is the better one.

| Variable | Effect |
|---|---|
| `SYNC_AUTO_MERGE` | `true` lets nightly pull requests merge themselves once required checks pass |
| `SYNC_LAG_DAYS` | days behind today for nightly; default `1` |
| `PUBLISH_ENABLED` | `true` allows the publish job to run |
| `SYNC_APP_ID` | the GitHub App's id |

| Secret | Where | What |
|---|---|---|
| `SYNC_APP_PRIVATE_KEY` | repository | the app's key; the app needs Contents, Pull requests, Issues and Workflows read/write and is installed on the repository |
| `SYNC_TOKEN` | repository | alternative: a fine-grained PAT with the same permissions, used when `SYNC_APP_ID` is unset |
| `CARGO_REGISTRY_TOKEN` | `crates-io` environment | a crates.io token; leave unset to use trusted publishing for `publish.yml` and environment `crates-io` |

Settings: default branch `main`; allow squash merging and auto-merge; let
Actions create pull requests; a ruleset on `main` and `rust-*` requiring a
pull request and the checks `lint`, `generated tree matches patches`,
`test (ubuntu-latest)`, `test (macos-latest)`, `test (windows-latest)`,
`conformance against pinned cargo`, `package` (the sync skips auto-merge on
a branch without required checks); environment `crates-io` with required
reviewers.

## Caveats

- Nightly builds print two `invalid float` errors: Cargo scans the whole
  upstream checkout and trips over its own malformed test fixtures. Noise.
- Cargo itself is not built from the patched tree; only the crate is
  exported.
- The Cargo team publishes the sibling crates around each release, sometimes
  after it. A stable sync builds the ones crates.io does not have yet from
  upstream's git and records them as `siblings_from_git` in `upstream.toml`,
  so the release branch and its pull request exist on release day; `verify`
  reproduces that choice from the pin. The publish workflow refuses such a
  tree: once the siblings are published, run `sync <version> --force` on the
  branch and merge that, which drops the git entries, then publish. `cargo
  package` is skipped for pre-release versions for the same reason.
