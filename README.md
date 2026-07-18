# Creatio DevHub

Creatio DevHub is a Windows desktop workbench for Creatio developers. It provides a visual,
job-based interface over the installed [clio](https://github.com/Advance-Technologies-Foundation/clio)
and Git command-line tools for managing environments, packages, applications, and source-controlled
workspaces.

DevHub does not connect directly to Creatio APIs and does not store Creatio passwords. Environment
registration and credentials remain owned by clio.

Current version: **0.2.1**

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
- Pull packages from Creatio into a workspace.
- Review changed files and colorized diffs.
- Commit changes and inspect Git history.
- Configure and push to a Git remote.
- Use the active GitHub CLI account for HTTPS pushes.
- Configure the global Git author name and email.
- Detect commits pushed by another user and block unsafe pushes until the workspace is updated.
- Add another package from the same Creatio environment to an existing workspace.

### Packages

- Browse and filter packages for an environment.
- Pull, push, lock, unlock, activate, deactivate, hotfix, change version, and delete packages.
- Install `.zip` and `.gz` package archives by file selection or drag and drop.
- Deploy a package from one registered environment to another.
- Optional target backup for supported package operations.
- Typed confirmation for destructive and deployment operations.

### Applications

- Browse Creatio applications using structured clio output.
- Display application name, code, version, and description.
- Deploy a whole application from one registered environment to another using
  `clio deploy-application`.
- Lock both environments during transfer to prevent overlapping mutations.

### Local catalog cache

- Persist application and package lists per environment in the DevHub app-data directory.
- Reopen pages and restart DevHub without waiting for another clio/server request.
- Display the cache timestamp so users know how current the data is.
- Use **Refresh** to explicitly retrieve the latest server state and replace the saved entry.
- Store catalog metadata only; credentials and clio environment secrets are never cached.

### Jobs and safety

- Every long-running operation is represented as a job with streamed output.
- Operations against the same environment are serialized.
- Credentials and secret command arguments are masked in logs.
- Safe phases can be cancelled, including termination of the Windows child-process tree.
- Server-side installation, compilation, Git push, and other unsafe phases cannot be terminated
  after they begin.
- Desktop notifications report completed jobs when DevHub is not focused.

### Updates

- Check GitHub Releases from **Settings → DevHub updates**.
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

## Prerequisites

- Windows 10 or Windows 11.
- Node.js 22 or a compatible current LTS version.
- Rust stable with the MSVC target.
- Visual Studio 2022 Build Tools with the C++ desktop workload.
- clio available on `PATH`.
- Git available on `PATH`.
- GitHub CLI available on `PATH` for the integrated GitHub account flow.
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

Release `v0.2.1` is live and the endpoint has been verified. The repository secret
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

The workflow at `.github/workflows/release.yml` builds the Windows release and publishes the
installer, signatures, and updater metadata.

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
