import { useEffect, useState } from "react";
import { getVersion } from "@tauri-apps/api/app";
import { open } from "@tauri-apps/plugin-dialog";
import { openUrl } from "@tauri-apps/plugin-opener";
import { ArrowUpFromLine, Copy, KeyRound, Monitor, Moon, RefreshCw, Sun } from "lucide-react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  Card,
  CardAction,
  CardContent,
  CardDescription,
  CardFooter,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Progress } from "@/components/ui/progress";
import { RadioGroup, RadioGroupItem } from "@/components/ui/radio-group";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Table, TableBody, TableCell, TableRow } from "@/components/ui/table";
import { checkForAppUpdate, installAppUpdate, Update } from "../../lib/appUpdate";
import ErrorNote from "../../lib/ErrorNote";
import { logError } from "../../lib/errorLog";
import { setThemeMode, ThemeMode, useTheme } from "../../lib/theme";
import {
  EnvSummary, getGithubStatus, getToolPaths, githubLoginWithToken, GithubStatus,
  listEnvironments, onJobLog, onJobUpdate, setDefaultEnvironment, setGitIdentity, setToolPath,
  startGithubLogin, ToolPath,
} from "../../lib/ipc";

/** GitHub's device-authorization page — where the one-time code is entered. */
const GH_DEVICE_URL = "https://github.com/login/device";

const THEMES: { value: ThemeMode; label: string; icon: typeof Sun }[] = [
  { value: "system", label: "System", icon: Monitor },
  { value: "light", label: "Light", icon: Sun },
  { value: "dark", label: "Dark", icon: Moon },
];

export default function SettingsPage() {
  const [environments, setEnvironments] = useState<EnvSummary[]>([]);
  const [selected, setSelected] = useState("");
  const [saved, setSaved] = useState("");
  const [error, setError] = useState("");
  // Show the failure inline and record it into the app-wide Errors view.
  const reportError = (e: unknown) => {
    const message = String(e);
    setError(message);
    logError("Settings", message);
  };
  const [busy, setBusy] = useState(false);
  const [github, setGithub] = useState<GithubStatus | null>(null);
  const [gitName, setGitName] = useState("");
  const [gitEmail, setGitEmail] = useState("");
  const [githubJob, setGithubJob] = useState<string | null>(null);
  const [githubNotice, setGithubNotice] = useState("");
  const [githubError, setGithubError] = useState("");
  // The one-time code gh prints during the browser flow, surfaced here so the
  // user doesn't have to hunt for it in the Jobs log.
  const [deviceCode, setDeviceCode] = useState("");
  const [deviceUrl, setDeviceUrl] = useState(GH_DEVICE_URL);
  const [codeCopied, setCodeCopied] = useState(false);
  const [token, setToken] = useState("");
  const [tokenBusy, setTokenBusy] = useState(false);
  const [tools, setTools] = useState<ToolPath[]>([]);
  const [toolError, setToolError] = useState("");
  const [appVersion, setAppVersion] = useState("");
  const [update, setUpdate] = useState<Update | null>(null);
  const [updateStatus, setUpdateStatus] = useState(
    "DevHub checks for new releases on its own and tells you in the header when one arrives. You can also check now.",
  );
  const [updateBusy, setUpdateBusy] = useState(false);
  const [updateProgress, setUpdateProgress] = useState<number | null>(null);
  const { mode } = useTheme();

  const load = async () => {
    try {
      const list = await listEnvironments();
      setEnvironments(list);
      const active = list.find((environment) => environment.isActive) ?? list[0];
      setSelected(active?.name ?? "");
      setSaved(active?.name ?? "");
    } catch (e) {
      reportError(e);
    }
  };

  useEffect(() => {
    load();
    refreshGithub();
    refreshTools();
    getVersion().then(setAppVersion);
  }, []);

  const checkForUpdate = async () => {
    setUpdateBusy(true);
    setUpdateProgress(null);
    setUpdateStatus("Checking GitHub Releases…");
    try {
      const available = await checkForAppUpdate();
      setUpdate(available);
      setUpdateStatus(available
        ? `Version ${available.version} is available.`
        : "You already have the latest version.");
    } catch (reason) {
      setUpdate(null);
      setUpdateStatus(`Update check failed: ${String(reason)}`);
    } finally {
      setUpdateBusy(false);
    }
  };

  const installUpdate = async () => {
    if (!update) return;
    setUpdateBusy(true);
    setUpdateStatus(`Downloading version ${update.version}…`);
    try {
      await installAppUpdate(update, (percent) => {
        setUpdateProgress(percent);
        if (percent === 100) setUpdateStatus("Update installed. Restarting DevHub…");
      });
    } catch (reason) {
      setUpdateStatus(`Update installation failed: ${String(reason)}`);
      setUpdateBusy(false);
    }
  };

  useEffect(() => {
    const unlisten = onJobUpdate((job) => {
      if (job.id === githubJob && ["succeeded", "failed", "cancelled"].includes(job.status)) {
        setGithubJob(null);
        setDeviceCode("");
        if (job.status === "succeeded") {
          setGithubNotice("GitHub sign-in completed.");
          refreshGithub();
        } else if (job.status === "cancelled") {
          setGithubNotice("GitHub sign-in was cancelled.");
        } else {
          setGithubError("GitHub sign-in failed. Open Jobs to review the output.");
        }
      }
    });
    return () => { unlisten.then((fn) => fn()); };
  }, [githubJob]);

  // gh streams the one-time code (and the device URL) to the job log; pull them
  // out and show them in place so signing in is one glance, not a trip to Jobs.
  useEffect(() => {
    if (!githubJob) return;
    const unlisten = onJobLog(({ id, line }) => {
      if (id !== githubJob) return;
      const code = line.match(/\b([0-9A-Z]{4}-[0-9A-Z]{4})\b/);
      if (code) {
        setDeviceCode(code[1]);
        setCodeCopied(false);
      }
      const url = line.match(/https:\/\/github\.com\/login\/device\S*/);
      if (url) setDeviceUrl(url[0]);
    });
    return () => { unlisten.then((fn) => fn()); };
  }, [githubJob]);

  const refreshGithub = async () => {
    try {
      const status = await getGithubStatus();
      setGithub(status);
      setGitName(status.gitName ?? status.accountName ?? status.login ?? "");
      setGitEmail(status.gitEmail ?? status.suggestedEmail ?? "");
    } catch (e) {
      setGithubError(String(e));
    }
  };

  const refreshTools = async () => {
    setToolError("");
    try {
      setTools(await getToolPaths());
    } catch (e) {
      setToolError(String(e));
    }
  };

  /// Pin a CLI to an explicit executable, for installs in a location DevHub
  /// does not know about. Cancelling the picker leaves the setting alone.
  const pickTool = async (program: string) => {
    const picked = await open({
      title: `Select the ${program} executable`,
      multiple: false,
      directory: false,
    });
    if (typeof picked !== "string") return;
    await applyToolPath(program, picked);
  };

  const applyToolPath = async (program: string, path: string) => {
    setToolError("");
    try {
      await setToolPath(program, path);
      await refreshTools();
      await refreshGithub();
    } catch (e) {
      setToolError(String(e));
    }
  };

  const save = async () => {
    if (!selected) return;
    setBusy(true);
    setError("");
    try {
      await setDefaultEnvironment(selected);
      setSaved(selected);
    } catch (e) {
      reportError(e);
    } finally {
      setBusy(false);
    }
  };

  const loginGithub = async () => {
    setGithubError("");
    setGithubNotice("");
    setDeviceCode("");
    setDeviceUrl(GH_DEVICE_URL);
    try {
      setGithubJob(await startGithubLogin());
      setGithubNotice("DevHub is opening a browser sign-in. When the one-time code appears below, enter it in your browser.");
    } catch (e) {
      setGithubError(String(e));
    }
  };

  const copyDeviceCode = async () => {
    try {
      await navigator.clipboard.writeText(deviceCode);
      setCodeCopied(true);
      window.setTimeout(() => setCodeCopied(false), 1500);
    } catch {
      // Clipboard can be unavailable; the code is still shown to type manually.
    }
  };

  // Sign in with a personal access token the user pastes. The token goes
  // straight to gh via the backend and is cleared from state right after.
  const loginWithToken = async () => {
    const value = token.trim();
    if (!value) {
      setGithubError("Paste a GitHub personal access token.");
      return;
    }
    setGithubError("");
    setGithubNotice("");
    setTokenBusy(true);
    try {
      await githubLoginWithToken(value);
      setToken("");
      setGithubNotice("Signed in to GitHub with a token.");
      await refreshGithub();
    } catch (e) {
      setGithubError(String(e));
    } finally {
      setTokenBusy(false);
    }
  };

  const saveIdentity = async () => {
    setGithubError("");
    setGithubNotice("");
    try {
      await setGitIdentity(gitName, gitEmail);
      setGithubNotice("Git commit identity saved globally.");
      await refreshGithub();
    } catch (e) {
      setGithubError(String(e));
    }
  };

  const updateFailed = updateStatus.startsWith("Update check failed")
    || updateStatus.startsWith("Update installation failed");

  return (
    <div className="mx-auto grid max-w-5xl gap-4 p-6">
      <div className="mb-1">
        <h1 className="text-xl font-semibold tracking-tight">Settings</h1>
        <p className="text-muted-foreground">
          Manage your environments, identity, and DevHub installation.
        </p>
      </div>

      <Card>
        <CardHeader>
          <CardTitle className="flex items-center gap-2 text-base">
            <ArrowUpFromLine className="size-4 text-primary" aria-hidden="true" />
            DevHub updates
          </CardTitle>
          <CardDescription>
            Securely download signed releases published from GitHub.
          </CardDescription>
          <CardAction>
            <Badge variant="secondary" className="font-mono">v{appVersion || "…"}</Badge>
          </CardAction>
        </CardHeader>
        <CardContent className="grid gap-3">
          {updateFailed
            ? <ErrorNote error={updateStatus} />
            : <p className="text-sm text-muted-foreground">{updateStatus}</p>}
          {updateProgress !== null && <Progress value={updateProgress} />}
          {update?.body && (
            <p className="rounded-md bg-muted p-3 text-sm whitespace-pre-wrap">{update.body}</p>
          )}
        </CardContent>
        <CardFooter className="gap-2">
          <Button variant="outline" onClick={checkForUpdate} disabled={updateBusy}>
            {updateBusy && !update ? "Checking…" : "Check for updates"}
          </Button>
          {update && (
            <Button onClick={installUpdate} disabled={updateBusy}>
              {updateBusy ? "Installing…" : `Install v${update.version} and restart`}
            </Button>
          )}
        </CardFooter>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle className="text-base">Appearance</CardTitle>
          <CardDescription>
            Choose a theme. System follows your Windows light/dark setting.
          </CardDescription>
        </CardHeader>
        <CardContent>
          <RadioGroup
            value={mode}
            onValueChange={(value) => setThemeMode(value as ThemeMode)}
            className="grid gap-3 sm:grid-cols-3"
          >
            {THEMES.map((theme) => (
              <Label
                key={theme.value}
                htmlFor={`theme-${theme.value}`}
                className="flex cursor-pointer items-center gap-3 rounded-lg border p-3 font-normal has-[[data-state=checked]]:border-primary has-[[data-state=checked]]:bg-accent/10"
              >
                <RadioGroupItem value={theme.value} id={`theme-${theme.value}`} />
                <theme.icon className="size-4 text-muted-foreground" aria-hidden="true" />
                {theme.label}
              </Label>
            ))}
          </RadioGroup>
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle className="text-base">Default environment</CardTitle>
          <CardDescription>
            This changes clio&apos;s active environment. DevHub uses it as the initial selection
            when creating workspaces, browsing packages, and starting environment operations.
          </CardDescription>
        </CardHeader>
        <CardContent className="grid gap-3">
          {environments.length === 0 ? (
            <p className="text-muted-foreground">Register an environment before choosing a default.</p>
          ) : (
            <div className="flex flex-wrap items-end gap-3">
              <div className="grid min-w-64 flex-1 gap-2">
                <Label htmlFor="default-env">Environment</Label>
                <Select value={selected} onValueChange={setSelected}>
                  <SelectTrigger id="default-env" className="w-full">
                    <SelectValue placeholder="Select an environment" />
                  </SelectTrigger>
                  <SelectContent>
                    {environments.map((environment) => (
                      <SelectItem key={environment.name} value={environment.name}>
                        {environment.name} — {environment.uri}
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
              </div>
              <Button disabled={busy || !selected || selected === saved} onClick={save}>
                {busy ? "Saving…" : "Save default"}
              </Button>
            </div>
          )}
          {saved && (
            <p className="text-sm text-muted-foreground">
              Current default: <strong className="text-foreground">{saved}</strong>
            </p>
          )}
          {error && <ErrorNote error={error} />}
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle className="text-base">GitHub and Git identity</CardTitle>
          <CardDescription>
            GitHub authentication controls which account pushes over HTTPS. Git name and email
            control the author recorded in new commits.
          </CardDescription>
        </CardHeader>
        <CardContent className="grid gap-4">
          {!github?.ghInstalled ? (
            <div className="grid gap-2">
              <ErrorNote error="DevHub could not start the GitHub CLI (gh)." />
              <p className="text-sm text-muted-foreground">
                If gh is installed, it was most likely added to PATH after you last signed in to
                Windows — DevHub inherits the sign-in PATH. Use Refresh status, or point DevHub at
                gh.exe directly under Command-line tools below. Otherwise install it from{" "}
                <code className="rounded bg-muted px-1 py-0.5 font-mono text-xs">
                  winget install GitHub.cli
                </code>.
              </p>
              {github?.ghError && (
                <p className="font-mono text-xs text-muted-foreground">{github.ghError}</p>
              )}
            </div>
          ) : github.authenticated ? (
            <p className="text-sm text-muted-foreground">
              Signed in to GitHub as <strong className="text-foreground">{github.login}</strong>
              {github.accountName ? ` (${github.accountName})` : ""}.
            </p>
          ) : (
            <p className="text-sm text-destructive">GitHub is not signed in.</p>
          )}

          <div className="flex flex-wrap gap-2">
            <Button
              variant="outline"
              onClick={loginGithub}
              disabled={!github?.ghInstalled || !!githubJob}
            >
              {githubJob
                ? "Waiting for sign-in…"
                : github?.authenticated ? "Switch GitHub account" : "Sign in to GitHub"}
            </Button>
            <Button variant="outline" onClick={() => { refreshGithub(); refreshTools(); }}>
              <RefreshCw aria-hidden="true" />
              Refresh status
            </Button>
          </div>

          {githubJob && deviceCode && (
            <div className="grid gap-2 rounded-lg border border-primary/40 bg-primary/5 p-3">
              <p className="text-sm">
                Enter this one-time code at{" "}
                <button
                  type="button"
                  className="font-medium underline underline-offset-2"
                  onClick={() => openUrl(deviceUrl)}
                >
                  github.com/login/device
                </button>
                :
              </p>
              <div className="flex flex-wrap items-center gap-2">
                <code className="rounded bg-muted px-3 py-1.5 font-mono text-lg tracking-[0.3em]">
                  {deviceCode}
                </code>
                <Button variant="outline" size="sm" onClick={copyDeviceCode}>
                  <Copy aria-hidden="true" />
                  {codeCopied ? "Copied" : "Copy code"}
                </Button>
                <Button variant="ghost" size="sm" onClick={() => openUrl(deviceUrl)}>
                  Open GitHub
                </Button>
              </div>
              <p className="text-xs text-muted-foreground">
                DevHub tried to open your browser automatically — if it didn't, use the link above.
                You can sign in there with your usual GitHub credentials.
              </p>
            </div>
          )}

          <div className="grid gap-2 rounded-lg border border-dashed p-3">
            <Label htmlFor="gh-token" className="flex items-center gap-1.5 text-sm font-medium">
              <KeyRound className="size-3.5" aria-hidden="true" />
              Or sign in with a token
            </Label>
            <p className="text-xs text-muted-foreground">
              Paste a GitHub personal access token with the{" "}
              <code className="rounded bg-muted px-1 py-0.5 font-mono">repo</code> scope. It goes
              straight to the GitHub CLI — DevHub never stores or logs it.
            </p>
            <div className="flex flex-wrap gap-2">
              <Input
                id="gh-token"
                type="password"
                autoComplete="off"
                spellCheck={false}
                className="min-w-64 flex-1 font-mono"
                placeholder="ghp_… or github_pat_…"
                value={token}
                onChange={(event) => setToken(event.target.value)}
                onKeyDown={(event) => { if (event.key === "Enter") { event.preventDefault(); loginWithToken(); } }}
              />
              <Button
                variant="outline"
                onClick={loginWithToken}
                disabled={!github?.ghInstalled || tokenBusy || !token.trim()}
              >
                {tokenBusy ? "Signing in…" : "Sign in with token"}
              </Button>
              <Button
                variant="ghost"
                onClick={() => openUrl("https://github.com/settings/tokens/new?scopes=repo&description=Creatio%20DevHub")}
              >
                Create a token…
              </Button>
            </div>
          </div>

          <div className="grid gap-3 sm:grid-cols-2">
            <div className="grid gap-2">
              <Label htmlFor="git-name">Git author name</Label>
              <Input
                id="git-name"
                value={gitName}
                onChange={(event) => setGitName(event.target.value)}
              />
            </div>
            <div className="grid gap-2">
              <Label htmlFor="git-email">Git author email</Label>
              <Input
                id="git-email"
                value={gitEmail}
                onChange={(event) => setGitEmail(event.target.value)}
              />
            </div>
          </div>

          {githubNotice && <p className="text-sm text-muted-foreground">{githubNotice}</p>}
          {githubError && <ErrorNote error={githubError} />}
        </CardContent>
        <CardFooter>
          <Button onClick={saveIdentity}>Save Git identity</Button>
        </CardFooter>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle className="text-base">Command-line tools</CardTitle>
          <CardDescription>
            DevHub drives these CLIs directly. It searches PATH — including the current system PATH,
            not just the one inherited at sign-in — and the usual install locations. Pin a path here
            if a tool lives somewhere else.
          </CardDescription>
        </CardHeader>
        <CardContent className="grid gap-3">
          <Table>
            <TableBody>
              {tools.map((tool) => (
                <TableRow key={tool.program}>
                  <TableCell className="font-medium">{tool.program}</TableCell>
                  <TableCell className="w-full">
                    {tool.path
                      ? <span className="font-mono text-xs break-all">{tool.path}</span>
                      : (
                        <span className="text-sm text-destructive">
                          Not found. Searched: {tool.searched.join(", ")}
                        </span>
                      )}
                    {tool.custom && <span className="text-xs text-muted-foreground"> (pinned)</span>}
                  </TableCell>
                  <TableCell className="text-right whitespace-nowrap">
                    <Button variant="ghost" size="sm" onClick={() => pickTool(tool.program)}>
                      Locate…
                    </Button>
                    {tool.custom && (
                      <Button
                        variant="ghost"
                        size="sm"
                        onClick={() => applyToolPath(tool.program, "")}
                      >
                        Reset
                      </Button>
                    )}
                  </TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
          {toolError && <ErrorNote error={toolError} />}
        </CardContent>
        <CardFooter>
          <Button variant="outline" onClick={refreshTools}>
            <RefreshCw aria-hidden="true" />
            Re-scan
          </Button>
        </CardFooter>
      </Card>
    </div>
  );
}
