# Creatio DevHub — Engineering Handoff

Last verified: **2026-07-20**

Current version: **0.4.0** (releases v0.3.1 and v0.3.2 shipped after this doc's milestone
table below was last written; see per-release commit messages for their scope. v0.4.0 is the
shadcn/ui design-system release described below.)

Repository: <https://github.com/SmitSuthar8834/creatio-devhub>

Latest release: <https://github.com/SmitSuthar8834/creatio-devhub/releases/tag/v0.4.0>

Website: <https://smitsuthar8834.github.io/creatio-devhub/> (branch `gh-pages`)

> **There is no v0.2.2.** That tag's workflow failed because the version bump wrote a UTF-8 BOM
> into `package.json` / `tauri.conf.json` — Windows PowerShell `Set-Content -Encoding utf8` writes
> a BOM. Use `[System.IO.File]::WriteAllText` with `UTF8Encoding($false)` (or an editor that
> preserves encoding) for JSON the toolchain parses. The broken tag was deleted, never reused.

## Design system: shadcn/ui migration (v0.4.0, 2026-07-20)

The UI was migrated from the hand-rolled `App.css` visual system to **shadcn/ui** (new-york
style, Radix + Tailwind v4, lucide icons). Shipped in v0.4.0.

- **Config**: `components.json` (style `new-york`, `cssVariables: true`, aliases `@/components`,
  `@/lib`, `@/hooks`), `.mcp.json` (shoogle registry MCP at `https://mcp.shoogle.dev/mcp`),
  `skills-lock.json` (skills `shadcn`, `search-registry-items`, `migrate-radix-to-base`).
- **Primitives** live in `src/components/ui/*` (button, card, badge, dialog, alert-dialog, input,
  label, select, tabs, table, progress, alert, checkbox, textarea, collapsible, separator,
  tooltip, scroll-area, dropdown-menu, sheet, sidebar, skeleton, radio-group, switch, sonner).
  Add more with `npx shadcn@latest add <name>` — it respects `components.json` and does **not**
  touch `src/index.css` theme tokens (verified).
- **Theme**: `src/index.css` is the single source of truth — `:root` (light) + `.dark` token sets
  supplied by the user, plus `@theme inline` mappings. Two token pairs were added beyond the
  supplied set: `--success`/`--warning` (+ `-foreground`), because env health and job outcomes
  need those voices. **No glassmorphism** (no `backdrop-blur`/`backdrop-filter` anywhere — only
  flat opacity tints). Fonts Inter + JetBrains Mono are bundled under `src/assets/fonts/`
  (no runtime network). Dark/light is class-based via `src/lib/theme.ts` (`system`/`light`/`dark`,
  follows OS, persisted to `localStorage`); the Settings → Appearance radio group drives it.
- **Toasts**: `sonner` via `src/components/ui/sonner.tsx`; `<Toaster>` is mounted in `App.tsx`, and
  `JobToaster.tsx` is a headless driver issuing one keyed toast per job.
- **Migrated screens**: every module now renders shadcn components — App shell/sidebar,
  Environments (+ Add/Edit dialogs), Jobs, Clio banner, ErrorNote, Settings, SQL, Packages,
  Applications, Workspaces, WorkspaceDetail, NewWorkspaceWizard, DeployFromGithubDialog.
- **`src/App.css` was deleted** — it was already orphaned (imported nowhere; `main.tsx` imports
  only `index.css`).
- **Validated**: `npx tsc --noEmit` clean; `npm run build` (tsc + vite) succeeds; `tauri dev`
  compiled and launched the desktop app cleanly. The frontend migration is the bulk of the change;
  the release also carries a `jobs.rs` "quiet job" flag (background health checks no longer raise
  toasts/desktop notifications) with a back-compat test for older `history.json` files.
- **Note for this dev box**: the shoogle MCP is project-scoped in `.mcp.json`; a Claude session
  must be started from `A:\PersonalComponents\creatio-devhub` for it to load. This migration used
  the official `npx shadcn@latest` CLI instead.

## Current state

Creatio DevHub is a Windows-first Tauri 2 desktop application that provides a visual workbench over
the installed clio, Git, and GitHub CLI tools. It manages Creatio environments, source-controlled
workspaces, packages, applications, background jobs, GitHub identity, and signed application
updates.

The main milestones are complete:

| Area | Status |
|---|---|
| Environment registration, default selection, ping/open, cliogate installation | Complete |
| Workspace creation/registration, pull, diff, commit, history, Git remote push | Complete |
| Empty-first workspace + selective add-package UI + create GitHub repo from app | Complete |
| Global job toaster (top-right running-job indicator) | Complete |
| Auto-captured catalog cache (background prefetch on launch / env change) | Complete |
| Deploy from GitHub (clone/refresh a repo and push-workspace into an env) | Complete |
| SQL query runner with CSV/Excel export (grid capped at 5,000 rows) | Complete |
| clio CLI management — install / update / repair, with failure diagnosis | Complete |
| Push workspace to Creatio with drift guard and backup controls | Complete |
| Package browsing, actions, archive installation, Git workspace bridge | Complete |
| Package deployment between registered environments | Complete |
| Whole-application deployment between registered environments | Complete |
| GitHub login/account switching, Git identity, remote-ahead conflict guard | Complete |
| Phase-aware jobs and safe cancellation | Complete |
| Persistent package/application catalog cache | Complete |
| Creatio-inspired original DevHub UI | Complete |
| Signed GitHub Releases updater | Complete and published |

## Verified release state

Release `v0.3.0` was built and published by GitHub Actions (2026-07-20).

- Workflow: `.github/workflows/release.yml` (run 29722892404, conclusion: success)
- Release: `DevHub v0.3.0`
- Update endpoint:
  `https://github.com/SmitSuthar8834/creatio-devhub/releases/latest/download/latest.json`
- Published artifacts:
  - `creatio-devhub_0.3.0_x64-setup.exe` (NSIS setup executable)
  - `creatio-devhub_0.3.0_x64-setup.exe.sig` (NSIS signature)
  - `creatio-devhub_0.3.0_x64_en-US.msi` (MSI installer)
  - `creatio-devhub_0.3.0_x64_en-US.msi.sig` (MSI signature)
  - `latest.json`
- `latest.json` reports version `0.3.0` with signed `windows-x86_64`, `windows-x86_64-nsis`,
  and `windows-x86_64-msi` entries.

Released so far: v0.2.1, v0.2.3 → v0.2.9, v0.3.0. The first published/verified release was
`v0.2.1`; the signed updater flow has been stable across every release since.

The repository Actions secret `TAURI_SIGNING_PRIVATE_KEY` is configured. The private key is not in
Git and must remain private. On the original development machine it is stored at:

```text
%USERPROFILE%\.tauri\creatio-devhub.key
```

Back this key up securely. Losing it prevents existing installations from accepting future
updates. Never print it in logs, add it to the repository, or upload it as a release asset.

## Empty-first workspaces + GitHub-from-app (v0.2.4, 2026-07-18)

New onboarding flow so a workspace no longer has to download every package up front:

- **Start empty**: `create_workspace_flow` takes a new `skip_restore: bool`. When set, it runs
  `create-workspace` + git init + an "Initial empty workspace" commit and **skips
  `restore-workspace`** (no packages pulled, no drift baseline recorded yet). The New Workspace
  wizard defaults to "Start empty"; "Pull everything now" is the opt-in.
- **Add packages selectively**: the pre-existing `add_package_to_workspace` command
  (`cfg-worspace` — note that misspelling is clio's own alias, verified against installed clio,
  aliases `cfgw` — + `restore-workspace`) is now surfaced in the UI. `WorkspaceDetail` has an
  "➕ Add package" action + a package-picker dialog fed by `list_packages`.
- **Create GitHub repo from the app**: new `create_github_repo` command runs
  `gh repo create <name> [--private|--public] --source <dir> --remote origin --push`, wiring
  `origin` and pushing the initial commit in one job. Exposed via a guidance banner and the
  History-tab remote bar (shown only when the workspace has no remote yet). Requires `gh` signed
  in (Settings → GitHub). Non-cancellable job (push is the unsafe phase).
- **Guidance banner + progress strip** in `WorkspaceDetail`
  (✅ Workspace → Packages → GitHub repo → Pushed) drives new users through the four steps.
- Touched: `src-tauri/src/workspaces.rs`, `src-tauri/src/lib.rs`, `src/lib/ipc.ts`,
  `src/modules/workspaces/NewWorkspaceWizard.tsx`, `src/modules/workspaces/WorkspaceDetail.tsx`,
  `src/App.css`. Validated: `tsc --noEmit` clean, `cargo check`/`cargo test --lib` = 13 passed,
  full `tauri dev` build ran and launched. Shipped in v0.2.4.

## Global job toaster + auto-captured catalog cache (v0.2.5, 2026-07-18)

Two additive UX features:

- **Global job toaster** (`src/modules/jobs/JobToaster.tsx`, mounted once in `App.tsx`): a
  top-right indicator that subscribes to `job-update` and seeds from `get_jobs` on mount. Running
  jobs stay pinned (pulsing ⏳ with label/phase/env); terminal jobs flash ✅/✗/⊘ and auto-dismiss
  after 6s. Clicking opens the Jobs screen. Purely frontend — no backend change.
- **Auto-captured catalog state** (`src-tauri/src/catalog.rs` → `prefetch_env_catalog`): a silent
  background thread runs read-only `list-packages` + `list-apps` for an environment and writes both
  into `catalog-cache.json`, then emits `catalog-updated`. Wired to fire on app launch (active env)
  and whenever the default env changes — `clio::set_default_environment` now takes `AppHandle` and
  emits `environment-changed`. Applications and Packages pages listen for `catalog-updated` and
  reload from the freshened cache live.
- Touched: `src-tauri/src/catalog.rs` (new), `src-tauri/src/lib.rs`, `src-tauri/src/clio.rs`,
  `src/lib/ipc.ts`, `src/App.tsx`, `src/App.css`, `src/modules/jobs/JobToaster.tsx` (new),
  `src/modules/applications/ApplicationsPage.tsx`, `src/modules/packages/PackagesPage.tsx`.
  Validated: `tsc --noEmit` clean, `cargo check`/`cargo test --lib` = 13 passed, full `tauri dev`
  build ran and launched, shell renders with no new runtime errors. Shipped in v0.2.5.

## Deploy from GitHub (v0.2.6, 2026-07-18)

GitHub → Creatio: install a workspace straight from a repository into an environment — e.g. to
restore a broken environment from known-good source, or move a repo's packages onto a fresh env.

- `github::list_github_repos` (`gh repo list --json …`) and `github::list_repo_branches`
  (`gh api repos/{owner}/{repo}/branches`) feed the picker.
- `workspaces::deploy_from_github` runs as one env-locked job: clone via `gh repo clone` (git
  clone fallback) into `<destParent>/<repo-leaf>`, or **hard-refresh** an existing clone
  (`fetch` + `checkout` + `reset --hard origin/<branch>`); verify the `.clio` folder; then
  `push-workspace -e <targetEnv>` (unsafe/non-cancellable, `--skip-backup true` when the user
  opts out). Optionally registers the clone as a workspace (dedup by path).
- Frontend: `src/modules/workspaces/DeployFromGithubDialog.tsx`, opened from a "Deploy from
  GitHub" button on the Workspaces page. Repo dropdown (from gh) with a graceful manual
  owner/name + clone-URL fallback when gh isn't authed; branch dropdown/loader; target env;
  destination + Browse; "backup first" and "keep as workspace" toggles; an overwrite warning.
- Touched: `src-tauri/src/github.rs`, `src-tauri/src/workspaces.rs`, `src-tauri/src/lib.rs`,
  `src/lib/ipc.ts`, `src/modules/workspaces/DeployFromGithubDialog.tsx` (new),
  `src/modules/workspaces/WorkspacesPage.tsx`. Validated: `tsc` clean, `cargo check`/`cargo test
  --lib` = 13 passed, full `tauri dev` build + launch, dialog renders with working manual
  fallback. Shipped in v0.2.6.

## SQL query runner + CSV/Excel export (v0.2.7, 2026-07-19)

- Added a dedicated SQL workspace with environment selection, a keyboard-friendly query editor,
  sticky result headers, row/column metadata, and a 2,000-row display cap.
- Queries run through `clio execute-sql-script`; credentials remain owned by clio and the target
  environment must have cliogate installed.
- Complete results can be exported directly to semicolon-delimited CSV or Excel (`.xlsx`).
- Added a dedicated sidebar icon, friendly clio error handling, and parser tests for quoted
  semicolon values.
- Validated with TypeScript, Vite production build, Rust compilation, and 15 passing Rust tests.
  A live read-only query and both export formats succeeded against `187559-crm-bundle`.

## Clear deployment failure summaries (v0.2.8, 2026-07-19)

- Failed jobs now show a plain-language outcome and a separate technical-details panel above the
  complete raw log.
- Creatio package deployments that finish only partially are identified explicitly. For example,
  a locally modified target schema is named, the skipped content is explained, and the original
  Creatio message plus clio exit code remain visible.
- Bare trailing messages such as `[ERR] - Error` no longer hide the meaningful failure that
  appeared earlier in the installation log.

## Saved SQL queries (v0.2.9, 2026-07-19)

- SQL queries can be named and stored locally in the DevHub webview profile.
- Saved queries retain their original environment and can be reopened, updated, copied under a
  new name, deleted, or run again immediately.
- A saved query is never silently redirected: rerunning is blocked when its original environment
  is no longer registered.

## clio CLI management + SQL row cap (v0.3.0, 2026-07-20)

DevHub now manages the clio CLI it depends on, and diagnoses clio failures instead of
surfacing raw output.

- **SQL grid cap raised 2,000 → 5,000 rows** (`DISPLAY_CAP` in `src-tauri/src/sql.rs`).
  Exports are still uncapped (clio writes the file) and `row_count` reports the true total,
  so the "grid shows first N" pill stays accurate.
- **`clio::clio_status`** parses `clio ver`: installed version (`clio:`), cliogate (`gate:`),
  the update notice (`clio X is available`), dotnet presence, and a `broken` flag when clio
  starts but can't load its own assemblies.
- **`clio::install_or_update_clio(mode)`** — `install` | `update` | `repair`. Repair does
  `dotnet tool uninstall clio -g` then `install`, which is the only thing that fixes a damaged
  install. Output is **captured, not streamed**, so failures can be diagnosed.
- **`clio::diagnose(output)`** maps known failures to actionable guidance and is also applied to
  `list_applications` / `list_packages` errors:
  - `Could not load file or assembly …` → damaged install, use Repair.
  - `Access to the path … is denied` / `failed to uninstall tool package` → clio's files are
    **locked by a running clio process**; finish jobs / close terminals, or run as admin.
    (This is the real cause of a failed `dotnet tool update clio -g` while DevHub is busy.)
  - cliogate missing, dotnet missing.
- **`src/modules/clio/ClioBanner.tsx`** — header strip: blocking red banner when clio is missing
  (Install) or damaged (Repair); dismissible blue banner when an update exists (Update + Repair),
  dismissal stored per version. Re-checks itself when a clio job settles.

Tests added: `clio::tests::diagnoses_real_clio_failures` (asserts against both real failures above)
and `parses_clio_version_and_update_notice`. Suite: **17 passing**.

## Parked work and known environment state

- Branch **`parked/team-locks`** (local, not pushed) holds a complete team-locks feature
  (DataService `UsrTeamLock`/`UsrTeamActivity` shipped in a bundled package, hard package-lock via
  `clio lock-package`, push conflict pre-check). **Parked deliberately**: true per-schema "only I
  can edit" enforcement is only available through Creatio's **SVN** native lock, which is not
  achievable via Git/GitHub (Creatio's native VC tooling is SVN-only; Git has no lock primitive).
- Test env `187559-crm-bundle` still holds leftover experiment objects
  (`UsrDevHubCollabPackage`, an empty `UsrDevHubCollab`, and unused `UsrDevHubLockItem` /
  `UsrDevHubActivityItem` in `Custom`). Harmless; cliogate is installed there and should stay.

## Website (branch `gh-pages`)

Live at <https://smitsuthar8834.github.io/creatio-devhub/> — a landing/download page.

- **Deployment**: the page source is a single standalone `index.html` on the orphan-style
  `gh-pages` branch. It is deployed with a `git worktree` checkout of `gh-pages` so `main` is
  never touched:

  ```bash
  git worktree add /path/tmp gh-pages
  cp index.html /path/tmp/ && cd /path/tmp && git add -A && git commit && git push origin gh-pages
  git worktree remove /path/tmp --force
  ```

- **Design**: dark-only ("OLED") theme, slate ground with a `#22C55E` accent, Inter +
  JetBrains Mono, inline SVG icons (no emoji), a stylized DevHub window in the hero, feature grid,
  workflow strip, changelog, requirements, and a download CTA.
- **Live data**: on load it queries the GitHub Releases API to show real installer download
  counts, latest version and release count, and to **re-target every download link at the newest
  release's assets** — so the buttons don't go stale between releases. There is a static fallback
  if the fetch fails.
- **Footer**: credit "Developed by Smit Suthar", support email `sutharsmit574@gmail.com`, and an
  explicit "independent tool, not affiliated with Creatio" disclaimer.
- GitHub Pages is configured to serve the `gh-pages` branch root.

## Architecture

```text
React + TypeScript UI
        |
        | typed Tauri invoke/events
        v
Rust command layer
        |
        +-- Job engine: logs, phases, cancellation, process trees, environment locks
        +-- clio process adapter: Creatio environment/package/application operations
        +-- Git adapter: status, diff, commit, history, remote synchronization
        +-- GitHub adapter: gh authentication and global Git identity
        +-- App-data state: workspace registry and catalog cache
```

There is no DevHub server component. The desktop application invokes local command-line tools and
those tools communicate with Creatio or GitHub.

## Branding / icons (added 2026-07-18, later session)

- Icon source sheet: `Downloads\Generated image 1.png` cropped into `src/assets/icons/`
  (logo-mark, logo-wordmark, environments, workspaces, packages, applications, jobs, settings,
  local-desktop, app-icon-512). White backgrounds converted to alpha (luminance ramp ≥215) so the
  icons sit on the dark sidebar.
- Sidebar (`App.tsx` NAV + brand) uses the PNG icons; active nav item applies
  `filter: brightness(0) invert(1)`. Wordmark (`logo-wordmark.png`, white bg) is reserved for
  README/light surfaces.
- Sidebar switched from navy to a light treatment (`--side-bg #fbfcfe`, dark ink, border-right)
  so the blue-gradient icons render in true color — matches the icon sheet's design language.
- App icons (`src-tauri/icons/*` — .ico/.icns/pngs/Android mipmaps) regenerated from
  `app-icon-512.png` via `npx tauri icon`.
- Shipped in v0.2.3. A dedicated SQL (database) sidebar icon was added in v0.2.7.

## Important design decisions

| Decision | Rationale |
|---|---|
| Tauri 2 + Rust + React/TypeScript | Small Windows installer and reliable native process control |
| Invoke installed clio instead of calling Creatio APIs | Reuses supported clio behavior and registered environments |
| Never store Creatio credentials in DevHub | clio remains the credential owner |
| Use the system Git and GitHub CLI | Reuses the user's credential manager and active GitHub account |
| Represent every long mutation as a job | Provides streamed logs, serialization, status, and safe cancellation |
| Lock jobs per environment | Prevents concurrent mutations against the same Creatio environment |
| Typed confirmation for destructive/deployment actions | Makes the intended target explicit |
| Cache catalogs, not credentials | Improves navigation speed without expanding secret storage |
| Verify updater signatures | Prevents installation of releases not signed by the project key |

## Persistent state

DevHub writes only operational metadata beneath the Tauri application-data directory:

- `workspaces.json` — registered workspace metadata.
- `catalog-cache.json` — package and application lists keyed by environment, with timestamps.

The catalog cache is used immediately when revisiting a page or restarting DevHub. **Refresh**
bypasses it and updates the entry from clio. Neither file contains Creatio passwords or OAuth
client secrets.

clio owns environment configuration and credentials in its own application settings. Do not expose
raw `clio env` output because it can include plaintext passwords.

## Job and cancellation rules

- Jobs queued behind an environment lock can be cancelled.
- Safe local phases may terminate the complete Windows child-process tree.
- Package download, workspace restore, and GitHub browser login can be cancellable.
- Installation, server compilation, package deletion/activation/locking, credential changes, and
  Git pushes become non-cancellable once their unsafe phase begins.
- Source and target locks are acquired in stable sorted order for cross-environment deployment.

Do not make unsafe phases cancellable without proving that interruption cannot leave Creatio,
Git, or local workspace state inconsistent.

## Source map

```text
src/
  App.tsx                         shell and navigation
  App.css                         shared visual system
  lib/ipc.ts                      typed frontend IPC contract
  modules/environments/           environment hub and registration
  modules/workspaces/             workspace list, wizard, changes, history,
                                  DeployFromGithubDialog
  modules/packages/               package manager and package deployment
  modules/applications/           application catalog and deployment
  modules/sql/SqlPage.tsx         SQL editor, results grid, saved queries, CSV/XLSX export
  modules/clio/ClioBanner.tsx     clio install / update / repair header banner
  modules/jobs/                   job list, logs, cancellation, JobToaster
  modules/settings/               defaults, GitHub identity, updater

src-tauri/src/
  lib.rs                          plugin/state/command registration
  jobs.rs                         job state, locks, streaming, cancellation
  clio.rs                         clio helpers, parsing, clio_status /
                                  install_or_update_clio / diagnose
  cache.rs                        persistent catalog cache
  catalog.rs                      background catalog prefetch (packages + apps)
  sql.rs                          SQL execution and CSV/XLSX export via clio
  workspaces.rs                   workspace registry and synchronization
  packages.rs                     package actions and deployment
  applications.rs                 application listing and deployment
  git.rs                          local Git and remote-ahead checks
  github.rs                       GitHub CLI auth, repo/branch listing, Git identity
  tools.rs                        locating clio/git/gh/dotnet (PATH, live registry
                                  PATH, well-known dirs, user overrides)
  diagnostics.rs                  failure-signature catalog: raw CLI output ->
                                  summary, cause, and resolution steps

src-tauri/tauri.conf.json         app, bundling, and updater configuration
src-tauri/capabilities/           frontend permissions
.github/workflows/release.yml     signed Windows release automation
dev.cmd                           development wrapper
build.cmd                         local production build wrapper
README.md                         user and contributor documentation
```

## Build and validation

This development machine has both a partial Visual Studio Community toolset and a complete Visual
Studio Build Tools installation. Raw Cargo commands may select the incomplete toolset and fail with
`LNK1104: cannot open msvcrt.lib`.

Use the wrappers:

```powershell
.\dev.cmd
.\build.cmd
```

Frontend validation:

```powershell
npx tsc --noEmit
npm run build
```

Rust validation from the Build Tools environment:

```powershell
cd src-tauri
cargo test --lib
```

Latest verified result (2026-07-20, v0.3.0):

- TypeScript check: passed
- Vite production build: passed
- Rust tests: **17 passed, 0 failed**
- GitHub v0.3.0 release workflow: passed (run 29722892404)
- Published artifacts: signed NSIS + MSI, both signatures, `latest.json`
- Public updater feed: verified reporting `0.3.0`

## Publishing the next release

1. Pull `main` and ensure the working tree is clean.
2. Implement and validate the change.
3. Increase the same version in **all four** files (a mismatch is easy to miss):
   - `package.json`
   - `src-tauri/Cargo.toml`
   - `src-tauri/tauri.conf.json`
   - `src-tauri/Cargo.lock` (the `creatio-devhub` package entry; `cargo` also rewrites it on the
     next build)

   Edit these with a tool that preserves encoding — a UTF-8 BOM in the JSON breaks the workflow
   (see the v0.2.2 note at the top). Verify with a BOM check before committing.
4. Update README and this handoff when behavior or operational requirements change.
5. Commit and push `main`.
6. Create and push the matching tag (example for the next patch after v0.3.0):

```powershell
git tag -a v0.3.1 -m "Creatio DevHub v0.3.1"
git push origin v0.3.1
```

7. Monitor **Publish DevHub release** in GitHub Actions.
8. Confirm the release contains both installers, both signatures, and `latest.json`.
9. Confirm the latest endpoint reports the new version before announcing the update.
10. Update the website (`gh-pages`) so the changelog and static fallbacks mention the new version.
    The download buttons re-target the newest release from the GitHub API at runtime, so they do
    not strictly need editing — but the changelog and fallback labels do.

Never reuse a version tag for different source. If a release fails, fix the workflow or secret and
rerun the same failed workflow only when its source/tag is unchanged.

## Known limitations and risks

- Mutating package operations have not all been exercised against a disposable live environment.
- Cross-environment package and whole-application deployments require final end-to-end validation
  against non-production targets before production use.
- Application deployment follows clio's descriptor and package dependency behavior; it is not a
  full Creatio ALM approval/promotion workflow.
- The workspace drift guard compares package names and versions. Server-side changes without a
  version change may not be detected.
- Workspace synchronization and some package operations require a compatible cliogate installation.
- DevHub currently depends on separately installed clio, Git, and GitHub CLI binaries. They are
  located by `tools.rs` rather than by `Command::new` alone: the inherited PATH is Explorer's
  login-time snapshot, so a tool installed after the last sign-in would otherwise read as "not
  installed" while working in any terminal. Resolution order is user override → inherited PATH →
  the live PATH read back from HKCU/HKLM via `reg.exe` → well-known install directories, with
  every `PATHEXT` variant tried so `.cmd`/`.bat` shims (scoop, npm) resolve too. Results are
  memoized until Refresh/Re-scan. Overrides are stored in app-data `tool-paths.json` and edited
  under Settings → Command-line tools.
- The update feed is public. Moving the source repository to private access requires a separate
  public release feed or an authenticated update service.
- Job history persists under app-data `jobs/` (`history.json`, capped at 200, + per-job log
  files written at completion). Jobs active during an app exit are shown as
  failed/"interrupted" on next start. Logs stream to memory during a run and are only flushed
  to disk at finish — a hard crash mid-job loses that job's log tail (record survives).
- Catalog cache invalidation is explicit via Refresh or selected successful mutations; it is not a
  real-time subscription to Creatio changes.
- **Windows only.** No macOS/Linux build exists — the release workflow produces NSIS + MSI on a
  Windows runner. Tauri and the clio/Git/gh dependencies are all cross-platform, so a macOS build
  is feasible, but it needs a `macos` runner job, a replacement for the Windows-specific
  process-tree cancellation, and Apple signing/notarization to avoid Gatekeeper warnings.
- **Open bug — "Start empty" workspace is not actually empty.** `create_workspace_flow` still
  passes `-e <env>` to `clio create-workspace`, so clio connects and populates the package
  selection instead of scaffolding an empty workspace (and it fails outright if the environment is
  unreachable or its credentials are stale). The correct invocation is
  `clio createw <name> --empty --directory <parent>` — no `-e`, no credentials. Deliberately
  deferred; note that `--empty` creates the subfolder itself, so the path handling needs adjusting
  with it.
- SQL execution runs **raw SQL** against the Creatio database through cliogate. `UPDATE`/`DELETE`
  are not sandboxed; the UI only warns. The grid is capped at 5,000 rows (exports are uncapped).
- clio itself is now installable/updatable/repairable from the app, but the **.NET SDK is still an
  external prerequisite** — without `dotnet` on PATH DevHub can only report the problem.

## Recommended next priorities

1. **Fix the "Start empty" workspace bug** (see Known limitations) — switch that path to
   `clio createw <name> --empty --directory <parent>` and adjust the destination handling.
2. Run a controlled end-to-end validation matrix using disposable packages/applications and
   non-production environments.
3. Add automated integration tests around cache invalidation and deployment job locking.
4. Add a **macOS build** if there is demand: macOS runner job, non-Windows job cancellation, and
   Apple signing/notarization (needs a paid Apple Developer account).
5. Add configurable clio executable path and log retention/export settings.
6. Add optional scheduled workspace refresh / tray behavior.
7. Clean up the leftover experiment objects on `187559-crm-bundle` when that env is no longer
   needed for testing.
8. Design a proper ALM promotion flow if approvals, environment policies, or release gates are
   required.

Done previously and no longer open: persist job history (`JobStore` in `jobs.rs` + Clear-history
button), and "decide whether to bundle clio" — clio stays an external tool, but DevHub now
installs/updates/repairs it from the header banner (v0.3.0).

## clio behaviours worth knowing (verified live, clio 8.1.x)

These cost real trial-and-error; the built-in `--help` is wrong in places.

**Output parsing**
- clio prepends `[INF]` / `[WAR]` log lines to almost every command, including JSON responses.
  A `[WAR] - clio X is available…` line starts with `[`, which breaks a JSON stream parser —
  find the first `{` before parsing (see `locks.rs`/`sql.rs` handling).
- An unreachable or restarting environment answers with an **HTML error page** (e.g. 401), not
  JSON. Detect `<!doctype` / `<html` and report it as transient rather than dumping the page.

**Commands that require cliogate** (`clio install-gate -e <env>`)
- `execute-sql-script`, `lock-package` / `unlock-package`, `install-sql-schema`.

**`execute-sql-script`** (backs the SQL screen)
- `--View csv|xlsx` **requires** `--DestinationPath`; only the default `table` view prints to
  stdout. CSV output is **semicolon-delimited** with CRLF line endings.

**`create-entity-schema`** (only used by the parked locks work)
- Columns go after **one** `--column` as space-separated specs; repeating the flag is rejected.
- `--title` is required, and **`--parent BaseEntity`** is required — without a parent clio creates
  a stray `UsrId` primary key and DataService inserts then fail on a not-null violation.
- A newly created schema is SELECT-able immediately but **INSERT fails until `restart-web-app`**.
- `delete-schema` does **not** drop the physical table, so a name can't cleanly be reused — pick a
  fresh schema name instead.

**DataService via `clio dataservice`**
- SELECT columns use `expressionType 0` (SchemaColumn); INSERT/DELETE **values** use
  `expressionType 2` (Parameter) — using 0 there fails with `NotSupportedException: SchemaColumn`.
- Writing a DateTime is locale-fragile; store timestamps as Text (e.g. epoch seconds).

**Tool maintenance**
- `dotnet tool update clio -g` fails with `Access to the path … is denied` when any clio process
  is running — including ones DevHub itself spawned. Finish jobs first, or use Repair.
- A damaged install reports `Could not load file or assembly …`; only uninstall + reinstall
  (Repair) fixes it.

## Handoff checklist

Before changing the project:

- Read `README.md` and this file.
- Confirm active GitHub account and repository access.
- Keep the updater private key outside Git.
- Preserve unrelated local/user changes.
- Verify clio command names against the installed clio version.
- Test destructive/deployment changes only against disposable non-production targets.
- Run TypeScript, frontend build, and Rust tests before publishing.
- Update both documentation files when the user-visible workflow changes.
