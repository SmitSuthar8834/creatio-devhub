import { invoke } from "@tauri-apps/api/core";
import { listen, UnlistenFn } from "@tauri-apps/api/event";

export interface EnvSummary {
  name: string;
  uri: string;
  authKind: "oauth" | "password" | "none";
  isNetCore: boolean;
  isActive: boolean;
  developerMode: boolean;
  maintainer: string;
}

export interface JobInfo {
  id: string;
  kind: string;
  env: string | null;
  displayCommand: string;
  status: "queued" | "running" | "cancelling" | "cancelled" | "succeeded" | "failed";
  phase: string;
  cancellable: boolean;
  cancelRequested: boolean;
  startedAt: number;
  finishedAt: number | null;
  exitCode: number | null;
  diagnosis: Diagnosis | null;
  /// Background work: shown in Jobs, but never toasted or notified.
  quiet: boolean;
}

/// A recognized failure: what happened, why, and what to do about it.
export interface Diagnosis {
  code: string;
  summary: string;
  cause: string;
  steps: string[];
}

export const diagnoseError = (text: string) => invoke<Diagnosis | null>("diagnose_error", { text });

export interface JobLogLine {
  id: string;
  line: string;
}

export const listEnvironments = () => invoke<EnvSummary[]>("list_environments");

export const setDefaultEnvironment = (name: string) =>
  invoke<void>("set_default_environment", { name });

export interface ClioStatus {
  installed: boolean;
  version: string | null;
  latest: string | null;
  updateAvailable: boolean;
  gateVersion: string | null;
  dotnet: string | null;
  /** clio runs but its install is damaged (missing assembly) — needs a repair. */
  broken: boolean;
}

export const getClioStatus = () => invoke<ClioStatus>("clio_status");

/** Install, update, or repair (uninstall + reinstall) clio. Returns a job id. */
export const installOrUpdateClio = (mode: "install" | "update" | "repair") =>
  invoke<string>("install_or_update_clio", { mode });

export interface GithubStatus {
  ghInstalled: boolean;
  ghPath: string | null;
  ghSearched: string[];
  ghError: string | null;
  authenticated: boolean;
  login: string | null;
  accountName: string | null;
  accountEmail: string | null;
  suggestedEmail: string | null;
  gitName: string | null;
  gitEmail: string | null;
}

export const getGithubStatus = () => invoke<GithubStatus>("github_status");

/// Where DevHub resolved each external CLI, and any user-pinned override.
export interface ToolPath {
  program: string;
  path: string | null;
  custom: string | null;
  searched: string[];
}

export const getToolPaths = () => invoke<ToolPath[]>("tool_paths");
export const setToolPath = (program: string, path: string) =>
  invoke<ToolPath>("set_tool_path", { program, path });
export const startGithubLogin = () => invoke<string>("start_github_login");
export const setGitIdentity = (name: string, email: string) =>
  invoke<void>("set_git_identity", { name, email });

export interface GithubRepo {
  nameWithOwner: string;
  name: string;
  url: string;
  defaultBranch: string;
  isPrivate: boolean;
}

export const listGithubRepos = () => invoke<GithubRepo[]>("list_github_repos");
export const listRepoBranches = (repo: string) =>
  invoke<string[]>("list_repo_branches", { repo });

export const deployFromGithub = (opts: {
  repo: string;
  cloneUrl: string;
  branch: string;
  destParent: string;
  targetEnv: string;
  skipBackup: boolean;
  register: boolean;
}) => invoke<string>("deploy_from_github", opts);

/// `quiet` marks background work the user did not start by hand. It still runs
/// and still appears in Jobs, but raises no toast and no desktop notification.
export const runClioJob = (kind: string, args: string[], env?: string, quiet = false) =>
  invoke<string>("run_clio_job", { kind, args, env: env ?? null, quiet });

export const getJobs = () => invoke<JobInfo[]>("get_jobs");

export const getJobLog = (id: string) => invoke<string[]>("get_job_log", { id });
export const cancelJob = (id: string) => invoke<void>("cancel_job", { id });

export const clearJobHistory = () => invoke<JobInfo[]>("clear_job_history");

export const onJobUpdate = (cb: (job: JobInfo) => void): Promise<UnlistenFn> =>
  listen<JobInfo>("job-update", (e) => cb(e.payload));

// Warm an environment's package + application caches in the background.
export const prefetchEnvCatalog = (env: string) =>
  invoke<void>("prefetch_env_catalog", { env });

export const onCatalogUpdated = (cb: (env: string) => void): Promise<UnlistenFn> =>
  listen<string>("catalog-updated", (e) => cb(e.payload));

export const onEnvironmentChanged = (cb: (env: string) => void): Promise<UnlistenFn> =>
  listen<string>("environment-changed", (e) => cb(e.payload));

export const onJobLog = (cb: (line: JobLogLine) => void): Promise<UnlistenFn> =>
  listen<JobLogLine>("job-log", (e) => cb(e.payload));

// ---------- applications ----------

export interface ApplicationInfo {
  id: string;
  name: string;
  code: string;
  version: string;
  description: string | null;
}

export interface CachedList<T> {
  items: T[];
  cachedAt: number;
  fromCache: boolean;
}

export const listApplications = (env: string, forceRefresh = false) =>
  invoke<CachedList<ApplicationInfo>>("list_applications", { env, forceRefresh });

/** Descriptor facts clio's list-apps omits. Needs SQL access (cliogate). */
export interface ApplicationExtras {
  code: string;
  maintainer: string;
  createdOn: string;
  modifiedOn: string;
  requiredPlatformVersion: string;
  packageCount: number;
}

export interface ApplicationPackage {
  name: string;
  version: string;
  maintainer: string;
}

export interface ApplicationPage {
  schemaName: string;
  packageName: string;
  parentSchemaName: string;
}

export interface ApplicationDetails {
  code: string;
  name: string;
  version: string;
  description: string;
  maintainer: string;
  createdOn: string;
  modifiedOn: string;
  installDate: string;
  lastUpdate: string;
  requiredPlatformVersion: string;
  marketplaceLink: string;
  helpLink: string;
  supportEmail: string;
  isHidden: string;
  needsUpdate: string;
  schemaNamePrefix: string;
  packages: ApplicationPackage[];
  pages: ApplicationPage[];
  /** Why part of the picture is missing — shown as a note, not an error. */
  notes: string[];
}

export const applicationExtras = (env: string) =>
  invoke<ApplicationExtras[]>("application_extras", { env });

export const applicationDetails = (env: string, code: string) =>
  invoke<ApplicationDetails>("application_details", { env, code });

export const deployApplicationBetweenEnvironments = (opts: {
  sourceEnv: string;
  targetEnv: string;
  appCode: string;
}) => invoke<string>("deploy_application_between_environments", opts);

// ---------- packages ----------

export interface PackageInfo {
  name: string;
  version: string;
  maintainer: string;
}

export interface PackageLockState {
  name: string;
  locked: boolean;
}

export type PackageAction =
  | "pull"
  | "push"
  | "lock"
  | "unlock"
  | "activate"
  | "deactivate"
  | "hotfix"
  | "version"
  | "delete";

export const listPackages = (env: string, forceRefresh = false) =>
  invoke<CachedList<PackageInfo>>("list_packages", { env, forceRefresh });

/** Lock state per package. Rejects when the environment has no cliogate — the
 *  Packages screen treats that as "unknown" rather than an error. */
export const packageLockStates = (env: string) =>
  invoke<PackageLockState[]>("package_lock_states", { env });

// ---------- environment comparison ----------

export type DiffStatus = "same" | "different" | "missingTarget" | "missingSource";

export interface DiffRow {
  category: "package" | "setting" | "feature" | "webservice" | "schema" | "lookup" | "lookupValue";
  key: string;
  /** null means absent on that side — not the same as an empty value. */
  source: string | null;
  target: string | null;
  status: DiffStatus;
  /** Code looks credential-shaped. A warning hint only; every setting value is
   *  masked regardless of this flag. */
  sensitive: boolean;
  /** Schema-level rows, present on packages whose hash differs. */
  detail: DiffRow[];
}

export interface DiffReport {
  sourceEnv: string;
  targetEnv: string;
  sourceCapturedAt: number;
  targetCapturedAt: number;
  rows: DiffRow[];
  counts: Record<string, number>;
}

export interface SnapshotInfo {
  env: string;
  capturedAt: number;
  sizeBytes: number;
  /** How long this environment's last successful capture took. 0 = never
   *  captured. Used instead of a built-in estimate, because a local install and
   *  a cloud tenant differ by minutes. */
  durationMs: number;
}

/** Long-running, read-only, cancellable. Returns a job id. */
export const captureEnvState = (env: string) =>
  invoke<string>("capture_env_state", { env });

export const listSnapshots = () => invoke<SnapshotInfo[]>("list_snapshots");

export const deleteSnapshot = (env: string) =>
  invoke<void>("delete_snapshot", { env });

export const diffEnvironments = (sourceEnv: string, targetEnv: string) =>
  invoke<DiffReport>("diff_environments", { sourceEnv, targetEnv });

export const exportDiffReport = (sourceEnv: string, targetEnv: string, path: string) =>
  invoke<void>("export_diff_report", { sourceEnv, targetEnv, path });

export const runPackageAction = (opts: {
  env: string;
  package: string;
  action: PackageAction;
  path?: string;
  value?: string;
  skipBackup?: boolean;
}) =>
  invoke<string>("run_package_action", {
    env: opts.env,
    package: opts.package,
    action: opts.action,
    path: opts.path ?? null,
    value: opts.value ?? null,
    skipBackup: opts.skipBackup ?? null,
  });

export const deployPackageBetweenEnvironments = (opts: {
  sourceEnv: string;
  targetEnv: string;
  package: string;
  skipBackup: boolean;
}) => invoke<string>("deploy_package_between_environments", opts);

// ---------- lookups / reference data ----------

export interface LookupInfo {
  name: string;
  table: string;
  package: string;
  hasDescription: boolean;
}

export interface LookupSnapshotInfo {
  env: string;
  capturedAt: number;
  sizeBytes: number;
  lookupCount: number;
}

/** Enumerate every lookup registered in an environment. Needs cliogate. */
export const listLookups = (env: string) => invoke<LookupInfo[]>("list_lookups", { env });

export const listLookupSnapshots = () =>
  invoke<LookupSnapshotInfo[]>("list_lookup_snapshots");

export const deleteLookupSnapshot = (env: string) =>
  invoke<void>("delete_lookup_snapshot", { env });

/** Read every lookup's values into a local snapshot. Read-only. Returns a job id. */
export const captureLookups = (env: string) =>
  invoke<string>("capture_lookups", { env });

/** Compare two captured lookup snapshots. Top rows are category "lookup"; each
 *  carries its differing values as "lookupValue" detail rows. */
export const diffLookups = (sourceEnv: string, targetEnv: string) =>
  invoke<DiffReport>("diff_lookups", { sourceEnv, targetEnv });

/** Dry-run: the idempotent upsert SQL that would migrate the given tables. */
export const buildLookupMigration = (sourceEnv: string, tables: string[]) =>
  invoke<string>("build_lookup_migration", { sourceEnv, tables });

/** Apply the selected lookups' source rows onto the target. Mutating. Returns a job id. */
export const migrateLookups = (opts: {
  sourceEnv: string;
  targetEnv: string;
  tables: string[];
  skipBackup: boolean;
}) => invoke<string>("migrate_lookups", opts);

// ---------- sql ----------

export interface SqlResult {
  columns: string[];
  rows: string[][];
  rowCount: number;
  truncated: boolean;
  /** True for UPDATE/INSERT/DDL, where returning no rows is the expected result. */
  statement: boolean;
}

export const runSql = (env: string, query: string) =>
  invoke<SqlResult>("run_sql", { env, query });

export const exportSql = (opts: { env: string; query: string; format: "csv" | "xlsx"; path: string }) =>
  invoke<void>("export_sql", opts);

// ---------- workspaces ----------

export interface WorkspaceSummary {
  id: string;
  name: string;
  path: string;
  env: string;
  appCode: string | null;
  createdAt: number;
  lastPull: number | null;
  lastPush: number | null;
  exists: boolean;
  branch: string | null;
  remote: string | null;
  dirtyCount: number;
}

export interface FileChange {
  path: string;
  status: string;
}

export interface Commit {
  hash: string;
  message: string;
  author: string;
  date: string;
}

export interface RemoteStatus {
  hasRemote: boolean;
  branch: string | null;
  ahead: number;
  behind: number;
}

export const listWorkspaces = () => invoke<WorkspaceSummary[]>("list_workspaces");

export const registerWorkspace = (path: string, env: string) =>
  invoke<WorkspaceSummary>("register_workspace", { path, env });

export const removeWorkspace = (id: string) => invoke<void>("remove_workspace", { id });

export const createWorkspaceFlow = (opts: {
  name: string;
  parentDir: string;
  env: string;
  appCode?: string;
  remoteUrl?: string;
  skipRestore?: boolean;
}) =>
  invoke<string>("create_workspace_flow", {
    name: opts.name,
    parentDir: opts.parentDir,
    env: opts.env,
    appCode: opts.appCode ?? null,
    remoteUrl: opts.remoteUrl ?? null,
    skipRestore: opts.skipRestore ?? false,
  });

export const pullWorkspace = (id: string) => invoke<string>("pull_workspace", { id });

export const addPackageToWorkspace = (id: string, packageName: string) =>
  invoke<string>("add_package_to_workspace", { id, package: packageName });

export const pushWorkspaceCloud = (id: string, force: boolean, skipBackup: boolean) =>
  invoke<string>("push_workspace_cloud", { id, force, skipBackup });
export const wsStatus = (id: string) => invoke<FileChange[]>("ws_status", { id });
export const wsDiff = (id: string, file: string) => invoke<string>("ws_diff", { id, file });
export const wsLog = (id: string) => invoke<Commit[]>("ws_log", { id });
export const wsCommit = (id: string, message: string) => invoke<string>("ws_commit", { id, message });
export const wsSetRemote = (id: string, url: string) => invoke<void>("ws_set_remote", { id, url });
export const wsRemoteStatus = (id: string) => invoke<RemoteStatus>("ws_remote_status", { id });
export const wsPushRemote = (id: string) => invoke<string>("ws_push_remote", { id });

export const createGithubRepo = (opts: {
  id: string;
  repoName: string;
  private: boolean;
  push: boolean;
}) =>
  invoke<string>("create_github_repo", {
    id: opts.id,
    repoName: opts.repoName,
    private: opts.private,
    push: opts.push,
  });

export const onWorkspacesChanged = (cb: () => void): Promise<UnlistenFn> =>
  listen("workspaces-changed", () => cb());
