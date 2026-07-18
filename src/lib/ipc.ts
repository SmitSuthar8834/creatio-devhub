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
}

export interface JobLogLine {
  id: string;
  line: string;
}

export const listEnvironments = () => invoke<EnvSummary[]>("list_environments");

export const setDefaultEnvironment = (name: string) =>
  invoke<void>("set_default_environment", { name });

export interface GithubStatus {
  ghInstalled: boolean;
  authenticated: boolean;
  login: string | null;
  accountName: string | null;
  accountEmail: string | null;
  suggestedEmail: string | null;
  gitName: string | null;
  gitEmail: string | null;
}

export const getGithubStatus = () => invoke<GithubStatus>("github_status");
export const startGithubLogin = () => invoke<string>("start_github_login");
export const setGitIdentity = (name: string, email: string) =>
  invoke<void>("set_git_identity", { name, email });

export const runClioJob = (kind: string, args: string[], env?: string) =>
  invoke<string>("run_clio_job", { kind, args, env: env ?? null });

export const getJobs = () => invoke<JobInfo[]>("get_jobs");

export const getJobLog = (id: string) => invoke<string[]>("get_job_log", { id });
export const cancelJob = (id: string) => invoke<void>("cancel_job", { id });

export const clearJobHistory = () => invoke<JobInfo[]>("clear_job_history");

export const onJobUpdate = (cb: (job: JobInfo) => void): Promise<UnlistenFn> =>
  listen<JobInfo>("job-update", (e) => cb(e.payload));

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
