# Creatio DevHub

Creatio DevHub is a cross-platform (Windows and macOS) desktop workbench for Creatio developers.
It provides a visual, job-based interface over the installed
[clio](https://github.com/Advance-Technologies-Foundation/clio)
and Git command-line tools for managing environments, packages, applications, and source-controlled
workspaces.

> **Platform status.** Windows and macOS installers are published for every release. The macOS build
> is a universal `.dmg` (Apple Silicon + Intel) and has been validated on a real Mac. It is not yet
> Apple-notarized, so the first launch needs a one-time right-click → Open (see Installing below).

DevHub does not connect directly to Creatio APIs and does not store Creatio passwords. Environment
registration and credentials remain owned by clio.

Current version: **0.2.9**

Repository: <https://github.com/SmitSuthar8834/creatio-devhub>

Latest release: <https://github.com/SmitSuthar8834/creatio-devhub/releases/latest>

## Implemented features

### Environments

- Read registered clio environments without exposing secrets.
- Register password or OAuth environments.
- Choose the default/active environment.
- Ping and open environments.
- Install or update cliogate.

### Workspaces and Git

- Create a workspace from a Creatio environment or register an existing folder.
- Start an **empty** workspace (scaffold + initial commit only, no packages downloaded) and add
  packages to it later, or pull everything from the environment up front.
- Add packages selectively from a picker, or pull all packages from Creatio into a workspace.
- Review changed files and colorized diffs.
- Commit changes and inspect Git history.
- **Create a GitHub repository directly from a workspace** (via the GitHub CLI), which wires the
  `origin` remote and pushes the initial commit in one step.
- A guidance banner walks new workspaces through the steps: Workspace → Packages → GitHub repo →
  Pushed.
- Configure and push to a Git remote.
- Use the active GitHub CLI account for HTTPS pushes.
- Configure the global Git author name and email.
- Detect commits pushed by another user and block unsafe pushes until the workspace is updated.
- Add another package from the same Creatio environment to an existing workspace.
- **Deploy from GitHub** — clone a repository at a chosen branch (or hard-refresh an existing
  clone) and install it into a target environment with `push-workspace`, for example to restore a
  broken environment from known-good source. Pick a repo and branch from your GitHub account (with
  a manual owner/name fallback), choose the target environment and destination, keep a backup, and
  optionally keep the clone as a workspace to keep iterating.

### Packages

- Browse packages for an environment, and filter by field: name or version in the search box,
  maintainer from its own dropdown (with per-maintainer counts). The two narrow together.
- Pull, push, lock, unlock, activate, deactivate, hotfix, change version, and delete packages.
- Install `.zip` and `.gz` package archives by file selection or drag and drop.
- Deploy a package from one registered environment to another.
- Optional target backup for supported package operations.
- Typed confirmation for destructive and deployment operations.

### Applications

- Browse Creatio applications using structured clio output.
- Display application name, code, version, and description, plus the developer, package count,
  required Creatio version, and last-updated date on each tile. Filter by developer as well as
  name, code, or version.
- Open **Details** for the full descriptor: dates, required platform version, schema prefix,
  marketplace and help links, the application's packages, and the pages it contributes. The
  descriptor reads through clio and SQL independently, so an environment without cliogate still
  shows what clio alone can report.
- Deploy a whole application from one registered environment to another using
  `clio deploy-application`.
- Lock both environments during transfer to prevent overlapping mutations.

### Compare environments

- Capture an environment's configuration state (`clio save-state`) — features, system settings,
  web services, and every package with per-package and per-schema hashes — into a local snapshot,
  then compare any two snapshots instantly without re-reading the environments.
- Expand a differing package to see exactly which schemas differ (the duplicate-element collision
  warning before a deployment).
- Compare **lookup / reference data** the same way: capture every lookup's values and diff them
  per value, keyed on Id.
- Setting values are masked per row until explicitly revealed; markdown exports omit them
  entirely. Snapshots can contain API keys, so treat the snapshot folder as sensitive.

### Migration

- **Lookup values**: copy unbound reference data (lookup rows added directly in an environment)
  between environments, keyed on Id so foreign keys stay valid. Preview the exact SQL first; a
  runnable rollback script is written before anything is changed.
- **Marketing content**: copy Campaigns, Bulk Emails, email designer templates, and dynamic
  content source→target with their original IDs, idempotently (existing IDs are skipped).
  Campaign **flow diagrams** are copied through SQL (they live in protected system tables) and
  campaigns are relinked to them; a finalize step clears Redis and restarts the target so the
  designer sees the flows.
- Pick individual **Campaign and Bulk Email records** to move, or move everything missing.
  Parents required by a selected record are included automatically.
- References the target cannot satisfy are resolved automatically: a missing lookup or contact
  reference is remapped to the target's same-named row when one exists, otherwise the field is
  cleared — and every such change is listed in the result, per record.
- Analyze before migrating: a gap table shows source/target/missing counts per entity, flags
  campaigns whose flow diagram is absent on the target, and warns about emails referencing
  images that only exist on the source.

### SQL runner

- Run raw SQL against a selected Creatio environment through clio and cliogate.
- Review result sets in a scrollable grid capped at 5,000 displayed rows.
- Report a statement that returns no rows — `UPDATE`, `INSERT`, DDL — as a plain success rather
  than an empty grid, with export disabled because there is nothing to write.
- Surface the database's own error when it rejects a statement, including the position hint. clio
  reports these with a success exit code, so DevHub reads the output rather than trusting it.
- Export the complete result directly to semicolon-delimited CSV or Excel.
- Save named queries locally, reopen them for editing, or rerun them against their original
  environment. DevHub blocks reruns if that environment is no longer registered.
- Keep credentials in clio; DevHub stages only the query in a temporary local file.
- Raw SQL can modify data, so review `UPDATE` and `DELETE` statements carefully.

### clio CLI management

- Detect whether the clio CLI is installed, which version, and whether a newer build exists.
- **Install clio** from a header banner when it is missing (requires the .NET SDK).
- **Update clio** when a newer version is published; the prompt can be dismissed per version.
- **Repair clio** (uninstall + reinstall) when an install is damaged — for example when clio
  reports `Could not load file or assembly …` and commands start failing.
- Explain known failures instead of showing raw output: a locked tool store (`Access to the
  path … is denied`) means a clio command is still running, so finish jobs and close terminals
  using clio, or run DevHub as administrator.

### Local catalog cache

- Persist application and package lists per environment in the DevHub app-data directory.
- Reopen pages and restart DevHub without waiting for another clio/server request.
- Display the cache timestamp so users know how current the data is.
- Use **Refresh** to explicitly retrieve the latest server state and replace the saved entry.
- **Automatically capture** the active environment's package and application state in the
  background on launch and whenever the default environment changes, so screens are ready
  instantly. Open Applications/Packages screens reload from the freshened cache automatically.
- Store catalog metadata only; credentials and clio environment secrets are never cached.

### Jobs and safety

- Every long-running operation is represented as a job with streamed output.
- Operations against the same environment are serialized.
- Credentials and secret command arguments are masked in logs.
- Safe phases can be cancelled, including termination of the whole child-process tree (via
  `taskkill /T` on Windows and a process-group signal on macOS/Linux).
- Server-side installation, compilation, Git push, and other unsafe phases cannot be terminated
  after they begin.
- Desktop notifications report completed jobs when DevHub is not focused.
- A global **job indicator** in the top-right corner shows running jobs from any screen (with
  phase and environment), flashes each job's outcome when it finishes, and links to the Jobs
  screen on click.
- Failed jobs summarize the outcome in plain language and separately show the meaningful technical
  error, including partial package deployments caused by locally modified Creatio schemas.
- A job whose output reports a Creatio server error (HTTP 500) is marked failed even when clio
  ends with a success exit code, so a deployment that failed on the server is never shown as green.

### Updates

- Announce a new release in the header on its own — DevHub checks shortly after launch and every
  six hours, so nobody has to remember to look. The notice can be dismissed per version, and a
  failed check stays silent so working offline is never interrupted.
- Check GitHub Releases on demand from **Settings → DevHub updates**.
- Show the available version, release notes, and download progress.
- Cryptographically verify downloaded updates.
- Install the update and restart DevHub.
- GitHub Actions release workflow builds Windows installers, signatures, and `latest.json`.

## Technology

- Tauri 2 and Rust for the desktop host and process management.
- React 19, TypeScript, and Vite for the interface.
- Installed `clio` CLI for Creatio operations.
- Installed `git` CLI for source control.
- GitHub CLI (`gh`) for account authentication and switching.

```text
React UI
   │ typed Tauri IPC
   ▼
Rust commands ── job engine ── streamed UI events
   │
   ├── clio CLI ── Creatio environments
   └── git / gh ── local workspace and GitHub
```

## Installing

Download from the [latest release](https://github.com/SmitSuthar8834/creatio-devhub/releases/latest):
the `.exe` (NSIS) or `.msi` on Windows, or the universal `.dmg` on macOS.

**Windows will warn you before it runs.** SmartScreen shows *"Windows protected your PC — unknown
publisher"*; choose **More info → Run anyway**. The installer is not signed with a Windows code
signing certificate, so Windows has no publisher identity to check. This is expected, not a sign
that anything is wrong with the download.

Releases *are* cryptographically signed, but with the Tauri updater key — that is what lets DevHub
verify an update genuinely came from this project before installing it. Windows does not use that
signature. Removing the SmartScreen warning would need a separate Authenticode certificate; see the
handoff for what that involves.

**On macOS**, download the universal `.dmg` and drag DevHub to Applications — no Terminal needed. The
app is not yet Apple-notarized, so Gatekeeper blocks the *first* launch with *"cannot be opened
because it is from an unidentified developer."* Right-click (or Control-click) the app in Finder and
choose **Open** once; after that it launches normally by double-click. Notarization (which would drop
that first-launch step) needs a paid Apple Developer account and is not set up yet — see the handoff.

## Prerequisites

- Windows 10/11, or macOS 12 (Monterey) or newer.
- Node.js 22 or a compatible current LTS version.
- Rust stable — the MSVC target on Windows; the default toolchain on macOS. A universal macOS build
  also needs the `aarch64-apple-darwin` and `x86_64-apple-darwin` targets (`build.sh` adds them).
- **Windows only:** Visual Studio 2022 Build Tools with the C++ desktop workload. macOS builds use
  the Xcode command-line tools (`xcode-select --install`).
- clio, Git, and (for the integrated GitHub account flow) GitHub CLI installed. DevHub finds them
  on `PATH`, in the current system PATH, or in their usual install directories; if a tool lives
  somewhere else, pin its path under Settings → Command-line tools.
- cliogate installed in Creatio environments that use workspace synchronization or package
  operations requiring it.

## Development

Install dependencies:

```powershell
npm install
```

Run the Tauri development application:

```powershell
.\dev.cmd
```

On macOS or Linux, use the shell wrappers instead (`dev.cmd`/`build.cmd` exist only to shim this
Windows box's Visual Studio toolchain — the Unix toolchains need no such wrapper):

```bash
./dev.sh
```

For a full from-scratch macOS setup and the verification checklist a first-time Mac tester should
run, see [`docs/mac-testing.md`](docs/mac-testing.md).

Run frontend validation:

```powershell
npx tsc --noEmit
npm run build
```

Run Rust tests from a Visual Studio developer environment:

```powershell
cd src-tauri
cargo test --lib
```

On the original development machine, use `dev.cmd` and `build.cmd`. These wrappers select the
complete Visual Studio Build Tools environment because another partial MSVC toolset is installed
on that machine.

## Production build

The project is currently configured for signed updater artifacts. The private signing key must
exist outside the repository:

```text
%USERPROFILE%\.tauri\creatio-devhub.key
```

Build installers:

```powershell
.\build.cmd
```

Outputs are written beneath:

```text
src-tauri\target\release\bundle\msi\
src-tauri\target\release\bundle\nsis\
```

Never commit or share the private key. Back it up securely: losing it prevents existing
installations from accepting future updates.

## Publishing an update

The update endpoint is configured as:

```text
https://github.com/SmitSuthar8834/creatio-devhub/releases/latest/download/latest.json
```

Release `v0.2.7` is published through the same signed workflow. The repository secret
`TAURI_SIGNING_PRIVATE_KEY` is already configured.

To publish the next version:

1. Keep releases publicly downloadable, or replace the update feed with an authenticated service.
2. Increase the version in `package.json`, `src-tauri/Cargo.toml`, and
   `src-tauri/tauri.conf.json`.
3. Commit and push the change.
4. Push the matching annotated version tag, for example:

```powershell
git tag -a v0.2.2 -m "Creatio DevHub v0.2.2"
git push origin v0.2.2
```

5. Wait for **Publish DevHub release** to succeed.
6. Verify the new release contains the installers, signatures, and `latest.json`.

The workflow at `.github/workflows/release.yml` is a Windows + macOS matrix: it builds the Windows
installers and the universal macOS bundle and publishes both, their signatures, and the merged
`latest.json` updater metadata to one release. A separate `.github/workflows/ci.yml` builds the
macOS bundle on every push without publishing, so cross-platform breakage is caught without a Mac on
hand. **Do not tag a release that ships a macOS build until that build has actually been run on a
Mac** — see the handoff's platform gate.

## Main workflows

### Add a package to Git

1. Open **Packages** and select the package environment.
2. Open the package actions and select **Add to workspace**.
3. Choose a clean workspace registered for the same environment.
4. Wait for the restore job.
5. Review the new files under **Workspaces → Changes**, commit, and push.

### Deploy a package

1. Open **Packages** and select the source environment.
2. Select **Deploy to environment** from the package actions.
3. Choose a different target environment.
4. Choose the backup behavior and type the target name.
5. Follow progress under **Jobs**.

### Deploy an application

1. Open **Applications** and select the source environment.
2. Select an application and click **Deploy**.
3. Choose a different target environment and type its name.
4. Follow transfer and installation under **Jobs**.

## Important limitations

- Live package and application deployment should first be validated against non-production
  environments and disposable test packages/applications.
- Application deployment uses clio's application descriptor and package dependencies; it is not a
  replacement for a full Creatio ALM promotion process.
- The drift guard compares package names and versions. A server-side schema edit without a package
  version change may not be detected.
- GitHub Releases updates currently require publicly downloadable release assets.
- DevHub currently relies on separately installed clio, Git, and GitHub CLI tools.
- Job history is held in memory and is cleared when DevHub restarts.

## Troubleshooting

DevHub recognizes common failures and shows the cause plus numbered resolution steps — in the
failure panel on the Jobs screen, and inline wherever an operation reports an error. Failures it
does not recognize still show the raw tool output, so nothing is hidden. Currently recognized:
damaged or locked clio installs, missing cliogate, a CLI that cannot be started, rejected Git
pushes, GitHub repository access and credential failures, and Creatio environments that are
unreachable, reject credentials, or refuse an operation.

**"DevHub could not start the GitHub CLI (gh)" when gh is installed.** A desktop app inherits the
PATH captured when you signed in to Windows, so a tool installed since then is invisible to it even
though it works in a terminal. DevHub also re-reads the current system PATH and checks the usual
install directories, so **Refresh status** normally resolves this without signing out. If it does
not — gh is installed somewhere unusual, or only as a `.cmd` shim — open **Settings →
Command-line tools**, which shows exactly where each CLI resolved to, and use **Locate…** to pin
the executable. The same section covers clio, Git, and dotnet.

## Repository map

```text
src/
  App.tsx                         application shell and navigation
  App.css                         shared visual system
  lib/ipc.ts                      typed frontend IPC surface
  modules/
    environments/
    workspaces/
    packages/
    applications/
    jobs/
    settings/

src-tauri/
  src/
    lib.rs                        Tauri initialization and command registration
    jobs.rs                       job lifecycle, locking, logs, and cancellation
    clio.rs                       safe clio environment access and shared parsing
    workspaces.rs                 workspace registry and synchronization
    packages.rs                   package operations and deployment
    applications.rs               application listing and deployment
    git.rs                        Git operations and remote conflict detection
    github.rs                     GitHub authentication and Git identity
    tools.rs                      locating the clio/git/gh/dotnet executables
    diagnostics.rs                known failures mapped to causes and fixes
  capabilities/default.json      frontend plugin permissions
  tauri.conf.json                 application, bundle, and updater configuration

.github/workflows/release.yml     signed GitHub Release publishing
dev.cmd                           development wrapper
build.cmd                         production build wrapper
HANDOFF.md                        chronological implementation status and engineering notes
```

## Additional documentation

See [HANDOFF.md](HANDOFF.md) for milestone history, verified behavior, machine-specific notes, and
the current list of work still pending.
