# Creatio DevHub — Handoff

_Last updated: 2026-07-18 (sessions 1–2: design → M1 → M2 → M3 → M4 complete)_

## What this is

A Tauri 2 desktop app ("GitHub Desktop for Creatio") that wraps the **clio CLI** into a visual
workbench: environment management, package operations, and two-way sync of Creatio Cloud package
source code with git. No hosting/server component — everything runs on the user's machine; the only
future "hosting" is a GitHub Releases page for installer distribution + auto-updates.

Full technical design (screens, flows, feature tiers, wireframes):
https://claude.ai/code/artifact/ff9dfd0e-d421-47cb-b0b5-3463a11baeaa

## Plan — milestones and progress

| Milestone | Scope | Status |
|---|---|---|
| **M1** | App skeleton, job engine (streaming, per-env locks, secret masking), Environment Hub (cards, Ping/Open/Install-gate, add-env dialog w/ password or OAuth), Jobs screen with live log | ✅ **Done & verified** |
| **M2** | Workspaces: create wizard (pull-from-env or register existing folder, native folder picker), pull-from-cloud (clean-tree guard), Changes tab (file list + colorized diff + commit bar), History tab (log, remote URL, git push) | ✅ **Done & verified** |
| **M3** | Push to Cloud: confirm dialog (backup toggle, uncommitted-changes warning) → streamed `clio push-workspace`; **drift guard** (package snapshot compare → Cancel / Pull first / Push anyway banner); desktop notifications on job finish when window unfocused | ✅ **Done & verified** |
| **M4** | Package Manager: grid per env (structured `clio list-packages -j` parser + fixture tests), actions pull-pkg/push-pkg/lock/unlock/activate/deactivate/set-version/hotfix/delete (typed confirmation for destructive), drag-drop .zip/.gz install, add an existing cloud package to a registered workspace for Git | ✅ **Done & verified** |
| **M5** | Polish & distribution: Settings screen (default environment + GitHub/Git identity done; clio path and log retention pending), remote Git conflict alerts, scheduled auto-pull + tray, clio sidecar bundling, updater via GitHub Releases | 🟨 **Started** |
| Later (Tier 2/3 from design) | App/ALM deploy between envs, env compare (save-state/show-diff), SQL & DataService console, live log streamer (`clio listen`), schema explorer, OAuth bootstrap wizard, licensing | ⬜ |

## Things done (session log)

1. **Research**: full clio command surface catalogued from the official repo; verified `git-sync` is
   just a script-runner (we orchestrate the underlying commands instead); captured real
   `clio packages` output for the drift parser.
2. **Design doc** published as artifact (link above) and kept updated (incl. §6.4 OAuth
   registration flow, §6.5 git remote & team clone flow).
3. **Toolchain bootstrapped on this machine**: VS 2022 BuildTools (VC workload), rustup + stable
   1.97.1 (first winget install was corrupt — reinstalled clean), Node 22 already present.
4. **Project scaffolded** (create-tauri-app, React-TS/Vite, identifier `com.qnt.creatiodevhub`).
5. **M1 built**: `jobs.rs`, `clio.rs`, Environments + Jobs UI. Verified: unit test parses the real
   local clio settings; app launched; UI checked in browser preview.
6. **M2 built**: `git.rs`, `workspaces.rs` (registry + create/pull flows), workspace UI, dialog
   plugin for folder picker. Verified: git roundtrip unit test; tsc + cargo clean.
7. **M3 built**: `push_workspace_cloud` + drift snapshots + notification plugin. Verified: snapshot
   parser fixture test (3/3 tests green), tsc clean.
8. **Release 0.1.0 produced**: standalone exe (9.3 MB), NSIS setup (2 MB), MSI (3.1 MB) under
   `src-tauri\target\release\bundle\`. A rebuild including M3 was started; if it isn't there,
   rerun `build.cmd`.
9. `HANDOFF.md` (this file) + persistent memory kept in sync; user rule saved: always write a
   handoff at session end / >75% context usage.
10. **M4 built**: Packages screen with environment selector, filterable package grid, pull,
    archive install, lock/unlock, activate/deactivate, hotfix, version update, and typed-confirm
    delete. Native `.zip`/`.gz` drag-drop and backup toggles are included. Version update is an
    orchestrated pull → `set-pkg-version` → push because clio has no direct remote set-version.
11. **M4 verified**: corrected command names against installed clio 8.1 help; live read-only
    `Qnovate_DevEnv` package query exposed empty package versions, so the implementation uses
    structured `list-packages -j` rather than relying on table columns. `tsc` clean, 6/6 Rust
    tests pass, and fresh EXE/MSI/NSIS bundles were built.
12. **M4 Git bridge added**: Package grid → Add to workspace filters to registered workspaces
    for the same environment, requires a clean Git tree, appends the selected package through
    `clio cfg-worspace --Packages`, runs `restore-workspace`, and opens the workspace Changes
    tab when the job succeeds. Existing package selections are preserved; no commit is created
    automatically. Verified against clio source behavior; 7/7 Rust tests pass. Because the
    normal release EXE was open during the final build, the newest EXE/MSI/NSIS artifacts are
    under `src-tauri\target-m4\release\`; close the running app before installing the new build.
13. **M5 started — Settings**: added a real Settings page with a default-environment selector.
    Saving calls `clio reg-web-app -a <name>`, verifies clio's active environment changed, and
    makes that environment the initial selection throughout DevHub. TypeScript and 7/7 Rust
    tests pass.
14. **GitHub collaboration safety**: Settings now shows the authenticated `gh` account, starts
    browser-based GitHub login/account switching, runs `gh auth setup-git`, and allows the global
    Git commit name/email to be aligned with that account. Workspace History fetches origin every
    60 seconds; if origin is ahead it warns that someone else pushed and blocks `git push` until
    the user pulls/rebases. A two-clone fixture verifies remote-ahead detection (8/8 tests).
15. **Environment-to-environment package deploy**: package actions now offer Deploy to
    environment. The user selects a different registered target, keeps or skips target backup,
    and types the target name to confirm. DevHub downloads/unzips the package from the source into
    a temporary folder and installs it into the target with streamed logs and ordered source/target
    environment locks.
16. **Phase-aware job cancellation**: jobs now expose phase, cancellable, and cancel-requested
    state; queued jobs can be cancelled before starting, and safe running phases terminate the
    complete Windows child-process tree. The Jobs screen shows Cancel only while safe. Package
    download, workspace restore, and GitHub browser login are cancellable; environment installs,
    package deletion/activation/locking, Git push, credential setup, and server-side compile phases
    are deliberately non-cancellable. Added a real process-tree termination test (9/9 tests pass).
    The normal release EXE was open, so this latest build is under `src-tauri\target-m4\release\`.
17. **Whole application deployment**: added an Applications screen backed by structured
    `clio list-apps -e <source> --json`. It lists application name/code/version/description and
    deploys a selected application with `clio deploy-application <code> -e <source> -d <target>`.
    The target must differ from the source and its name must be typed to confirm. Source and target
    locks prevent overlapping environment mutations; deployment is deliberately non-cancellable
    once transfer/install starts. TypeScript is clean and 10/10 Rust tests pass.
18. **Signed self-updates and visual refresh (v0.2.0)**: Settings can check the
    `SmitSuthar8834/creatio-devhub` GitHub Releases feed, show release notes/download progress,
    install a cryptographically signed update, and restart DevHub. The release workflow builds
    Windows installers and publishes `latest.json`. The signing private key lives only at
    `%USERPROFILE%\.tauri\creatio-devhub.key`; back it up securely and add its contents to the
    repository secret `TAURI_SIGNING_PRIVATE_KEY` before publishing. The shell now uses an original
    DevHub identity with a Creatio-inspired navy/blue palette, rounded white cards, a compact top
    bar, and clearer spacing. No Creatio logo is included.
19. **Persistent catalog cache (v0.2.1)**: application and package lists are cached per
    environment in app-data `catalog-cache.json`. Returning to a page or restarting DevHub now
    renders saved data without another clio/server call. Each page shows the saved timestamp;
    Refresh bypasses the cache and replaces it. The cache contains catalog metadata only, never
    credentials. TypeScript is clean and 11/11 Rust tests pass.

## Key decisions (do not re-litigate)

| Decision | Reason |
|---|---|
| Tauri 2 (Rust) + React/TS/Vite | Small installer, native process control; Windows-first |
| Shell out to `clio` CLI as child processes | Stays in sync with user's clio (8.1.0.84+); app never talks to Creatio endpoints directly |
| App stores **zero credentials** | clio's `%LOCALAPPDATA%\creatio\clio\appsettings.json` owns environments/secrets (NB: plaintext passwords in that file — never render or log secret fields; OAuth is the recommended path; `-p/--Password/--ClientSecret` values are masked `•••` in job logs) |
| Git via system `git` CLI, `GIT_TERMINAL_PROMPT=0` | Reuses user's credential manager/SSH; auth prompts become fast failures with guidance |
| Workspace registry = `workspaces.json` in app-data | The file is the single source of truth; `WsState` serializes read-modify-write; "Remove" never deletes folders |
| Every operation is a "job" | `job-log`/`job-update` Tauri events stream to UI; jobs serialized per environment via named locks |
| clio `git-sync` command NOT used | We run `create-workspace`/`restore-workspace`/`push-workspace` + git directly for better progress reporting |
| Drift guard = package name\|version snapshot diff | Cheap, no extra services. Limitation: cloud schema edits without a version bump are invisible — v2 should query `SysPackage.ModifiedOn` via `clio dataservice` |

## Build & run — MACHINE QUIRK, READ FIRST

This machine's **VS Community has a partial MSVC 14.44 toolset (linker but no libs)** which rustc
prefers over the complete BuildTools install → raw `cargo build` fails with
`LNK1104: cannot open msvcrt.lib`.

**Always build via the wrappers** (they call BuildTools `vcvars64.bat` and prepend `~/.cargo/bin`):

- `dev.cmd` — run in dev mode (vite + cargo watcher, hot reload)
- `build.cmd` — production build (exe + NSIS setup + MSI in `src-tauri\target\release\bundle\`)

Version 0.2.0 and later can self-update from signed GitHub Releases once the repository and its
release secret are configured. Local builds still use `build.cmd`. Toolchain: rustc/cargo 1.97.1
(MSVC), Node 22, VS 2022 BuildTools VC workload.

Verify after changes: `cargo test --lib` (3 tests) from a vcvars shell + `npx tsc --noEmit`.

## Code map

```
src-tauri/src/
  lib.rs         plugin + command registration, WsState setup
  jobs.rs        JobState: create_job/log/finish (finish sends the unfocused-window
                 notification), stream_process, per-env locks, secret masking
  clio.rs        settings_path/list_environments (safe fields only),
                 clio_capture, parse_packages_snapshot, packages_snapshot
  cache.rs       persistent per-environment application/package catalog cache
  git.rs         git()/git_ok() sync helpers, status/log/current_branch/remote_url
  packages.rs    structured package-list parser, guarded package action jobs,
                 version pull/edit/push orchestration
  applications.rs structured application-list parser and source-to-target application deploy job
  workspaces.rs  registry (WsState), create_workspace_flow, pull_workspace,
                 push_workspace_cloud (drift guard), ws_* git commands
src/
  lib/ipc.ts     typed invoke wrappers + event subscriptions (the single IPC surface)
  modules/environments|workspaces|packages|applications|jobs/ pages; App.tsx = sidebar/routing
dev.cmd / build.cmd   build wrappers (see quirk above)
```

## Known issues / gotchas

- `clio env` dumps full settings **including plaintext passwords** — never surface raw output.
- clio output is human text; every parser lives in `clio.rs` with fixture tests. `clio packages`
  rows: `Name  Version  Maintainer`, with `[WAR]/[INF]` noise lines to skip. Note: piping clio
  through `Select-Object -First N` breaks its exit code (pipe close → 255) — capture fully.
- `restore-workspace`/`push-workspace` require **cliogate ≥ 2.0** on the environment ("Install
  gate" button on the env card).
- First `git push` may fail until the user authenticates once in a terminal; job log explains.
- Push-to-cloud is not safely cancellable once installation starts; the Jobs screen explains the
  unsafe phase and disables cancellation.
- **Env registrations**: `Trail-187417` (the active default) currently returns Unauthorized (stale
  password); `Qnovate_DevEnv` verified working (used for the packages fixture). `dev-834` is a
  local IIS env (http://QWI008:40000). Primary test target per user: dev-834 / QntProjectHub.
- Real end-to-end workspace create→pull→push against a live environment has NOT been exercised yet
  — that's the first thing to do in the next session (needs cliogate on the chosen env).
- M4 package reads were checked against `Qnovate_DevEnv`, but mutating package actions were not
  executed against a live environment during implementation. Test them first on a disposable
  package; lock/unlock requires cliogate >= 2.0.0.42.
- Environment-to-environment package and whole-application deploys have not been executed against
  a live target. First validate with a disposable package/application and a non-production target.
  Application deployment follows clio's application descriptor and package dependencies; it is
  separate from a Creatio ALM promotion workflow.
- The repo is **not under git yet** (user hasn't asked; suggest `git init` + initial commit).
