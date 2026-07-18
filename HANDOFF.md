# Creatio DevHub — Engineering Handoff

Last verified: **2026-07-18**

Current version: **0.2.1**

Repository: <https://github.com/SmitSuthar8834/creatio-devhub>

Latest release: <https://github.com/SmitSuthar8834/creatio-devhub/releases/tag/v0.2.1>

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

Release `v0.2.1` was built and published by GitHub Actions.

- Workflow: `.github/workflows/release.yml`
- Release: `DevHub v0.2.1`
- Update endpoint:
  `https://github.com/SmitSuthar8834/creatio-devhub/releases/latest/download/latest.json`
- Endpoint verification: HTTP 200
- Published artifacts:
  - NSIS setup executable
  - NSIS signature
  - MSI installer
  - MSI signature
  - `latest.json`
- `latest.json` reports version `0.2.1` and includes signed NSIS/MSI Windows entries.

The repository Actions secret `TAURI_SIGNING_PRIVATE_KEY` is configured. The private key is not in
Git and must remain private. On the original development machine it is stored at:

```text
%USERPROFILE%\.tauri\creatio-devhub.key
```

Back this key up securely. Losing it prevents existing installations from accepting future
updates. Never print it in logs, add it to the repository, or upload it as a release asset.

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
  modules/workspaces/             workspace list, wizard, changes, history
  modules/packages/               package manager and package deployment
  modules/applications/           application catalog and deployment
  modules/jobs/                   job list, logs, cancellation
  modules/settings/               defaults, GitHub identity, updater

src-tauri/src/
  lib.rs                          plugin/state/command registration
  jobs.rs                         job state, locks, streaming, cancellation
  clio.rs                         safe clio helpers and shared parsing
  cache.rs                        persistent catalog cache
  workspaces.rs                   workspace registry and synchronization
  packages.rs                     package actions and deployment
  applications.rs                 application listing and deployment
  git.rs                          local Git and remote-ahead checks
  github.rs                       GitHub CLI authentication and Git identity

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

Latest verified result:

- TypeScript check: passed
- Vite production build: passed
- Rust tests: **11 passed, 0 failed**
- Local signed v0.2.1 MSI/NSIS artifacts: produced
- GitHub v0.2.1 release workflow: passed
- Public updater feed: verified

## Publishing the next release

1. Pull `main` and ensure the working tree is clean.
2. Implement and validate the change.
3. Increase the same version in:
   - `package.json`
   - `src-tauri/Cargo.toml`
   - `src-tauri/tauri.conf.json`
4. Update README and this handoff when behavior or operational requirements change.
5. Commit and push `main`.
6. Create and push the matching tag:

```powershell
git tag -a v0.2.2 -m "Creatio DevHub v0.2.2"
git push origin v0.2.2
```

7. Monitor **Publish DevHub release** in GitHub Actions.
8. Confirm the release contains both installers, both signatures, and `latest.json`.
9. Confirm the latest endpoint reports the new version before announcing the update.

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
- DevHub currently depends on separately installed clio, Git, and GitHub CLI binaries.
- The update feed is public. Moving the source repository to private access requires a separate
  public release feed or an authenticated update service.
- Job history is currently in memory and is not restored after restarting DevHub.
- Catalog cache invalidation is explicit via Refresh or selected successful mutations; it is not a
  real-time subscription to Creatio changes.

## Recommended next priorities

1. Run a controlled end-to-end validation matrix using disposable packages/applications and
   non-production environments.
2. Add automated integration tests around cache invalidation and deployment job locking.
3. Add configurable clio executable path and log retention settings.
4. Persist job history and provide log cleanup/export.
5. Decide whether to bundle clio or keep it as an external prerequisite.
6. Add optional scheduled workspace refresh/tray behavior.
7. Design a proper ALM promotion flow if approvals, environment policies, or release gates are
   required.

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
