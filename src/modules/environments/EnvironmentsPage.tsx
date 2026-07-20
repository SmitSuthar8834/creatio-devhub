import { useEffect, useRef, useState } from "react";
import { ExternalLink, Plus, RefreshCw } from "lucide-react";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog";
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
import { EnvSummary, JobInfo, listEnvironments, onJobUpdate, runClioJob } from "../../lib/ipc";
import AddEnvironmentDialog from "./AddEnvironmentDialog";
import EditEnvironmentDialog from "./EditEnvironmentDialog";

type EnvStatus = "unknown" | "checking" | "online" | "offline";

export default function EnvironmentsPage() {
  const [envs, setEnvs] = useState<EnvSummary[]>([]);
  const [status, setStatus] = useState<Record<string, EnvStatus>>({});
  const [pingJobs, setPingJobs] = useState<Record<string, string>>({}); // jobId -> env
  const [showAdd, setShowAdd] = useState(false);
  const [editing, setEditing] = useState<EnvSummary | null>(null);
  const [confirmRemove, setConfirmRemove] = useState<EnvSummary | null>(null);
  // Guards the launch health check so it runs once per mount, not on every
  // environment-list refresh.
  const checkedOnLoad = useRef(false);

  const refresh = () => listEnvironments().then(setEnvs).catch(console.error);

  useEffect(() => {
    refresh();
  }, []);

  /// `quiet` keeps bulk checks out of the toaster and off the desktop — the
  /// status badge on each card is the feedback for those.
  const ping = async (env: EnvSummary, quiet = false) => {
    setStatus((s) => ({ ...s, [env.name]: "checking" }));
    const jobId = await runClioJob("ping-app", ["ping", "-e", env.name], env.name, quiet);
    setPingJobs((p) => ({ ...p, [jobId]: env.name }));
  };

  /// Check every registered environment. Runs once when the screen first has
  /// environments, and on demand from "Check all". Always quiet: one toast per
  /// environment on every launch is exactly the noise this avoids.
  const checkAll = (list: EnvSummary[]) => list.forEach((env) => ping(env, true));

  useEffect(() => {
    if (!checkedOnLoad.current && envs.length > 0) {
      checkedOnLoad.current = true;
      checkAll(envs);
    }
  }, [envs]);

  useEffect(() => {
    const un = onJobUpdate((job: JobInfo) => {
      const envName = pingJobs[job.id];
      if (envName && ["succeeded", "failed", "cancelled"].includes(job.status)) {
        setStatus((s) => ({
          ...s,
          [envName]: job.status === "succeeded" ? "online" : job.status === "failed" ? "offline" : "unknown",
        }));
      }
      if ((job.kind === "reg-web-app" || job.kind === "unreg-web-app") && job.status === "succeeded") {
        refresh();
      }
    });
    return () => {
      un.then((f) => f());
    };
  }, [pingJobs]);

  const open = (env: EnvSummary) => runClioJob("open-web-app", ["open", "-e", env.name], env.name);

  const installGate = (env: EnvSummary) =>
    runClioJob("install-gate", ["install-gate", "-e", env.name], env.name);

  const remove = async (env: EnvSummary) => {
    setConfirmRemove(null);
    await runClioJob("unreg-web-app", ["unreg-web-app", "-e", env.name], env.name);
  };

  const statusBadge = (s: EnvStatus) => {
    switch (s) {
      case "online":
        return <Badge className="bg-success/15 text-success border-transparent">online</Badge>;
      case "offline":
        return <Badge variant="destructive">unreachable</Badge>;
      case "checking":
        return <Badge variant="secondary">checking…</Badge>;
      default:
        return <Badge variant="outline" className="text-muted-foreground">not checked</Badge>;
    }
  };

  const checking = Object.values(status).some((s) => s === "checking");

  return (
    <div className="mx-auto max-w-5xl p-6">
      <div className="mb-5 flex items-center justify-between gap-3">
        <h1 className="text-xl font-semibold tracking-tight">Environments</h1>
        <div className="flex gap-2">
          {envs.length > 0 && (
            <Button variant="outline" onClick={() => checkAll(envs)} disabled={checking}>
              <RefreshCw className={checking ? "animate-spin" : ""} aria-hidden="true" />
              {checking ? "Checking…" : "Check all"}
            </Button>
          )}
          <Button onClick={() => setShowAdd(true)}>
            <Plus aria-hidden="true" />
            Add environment
          </Button>
        </div>
      </div>

      {envs.length === 0 && (
        <p className="text-muted-foreground">No environments registered yet. Add one to get started.</p>
      )}

      <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
        {envs.map((env) => (
          <Card key={env.name}>
            <CardHeader>
              <CardTitle className="text-base">{env.name}</CardTitle>
              <CardDescription className="truncate" title={env.uri}>
                {env.uri}
              </CardDescription>
              <CardAction>{statusBadge(status[env.name] ?? "unknown")}</CardAction>
            </CardHeader>
            <CardContent className="flex flex-wrap gap-1.5">
              {env.isActive && <Badge variant="secondary">default</Badge>}
              <Badge variant="outline">
                {env.authKind === "oauth" ? "OAuth" : env.authKind === "password" ? "password auth" : "no auth"}
              </Badge>
              {env.developerMode && <Badge variant="outline" className="text-muted-foreground">dev mode</Badge>}
            </CardContent>
            <CardFooter className="flex flex-wrap gap-2">
              <Button size="sm" variant="outline" onClick={() => ping(env)}>Ping</Button>
              <Button size="sm" variant="outline" onClick={() => open(env)}>
                Open <ExternalLink aria-hidden="true" />
              </Button>
              <Button size="sm" variant="outline" onClick={() => setEditing(env)} title="Change the URL or credentials">
                Settings
              </Button>
              <Button
                size="sm"
                variant="outline"
                onClick={() => installGate(env)}
                title="Install or update cliogate (required for workspace sync)"
              >
                Install gate
              </Button>
              <Button
                size="sm"
                variant="ghost"
                className="text-destructive hover:text-destructive"
                onClick={() => setConfirmRemove(env)}
                title="Remove this environment from clio's registry"
              >
                Remove
              </Button>
            </CardFooter>
          </Card>
        ))}
      </div>

      {showAdd && (
        <AddEnvironmentDialog onClose={() => setShowAdd(false)} onSubmitted={() => setShowAdd(false)} />
      )}
      {editing && (
        <EditEnvironmentDialog
          env={editing}
          onClose={() => setEditing(null)}
          onSubmitted={() => setEditing(null)}
        />
      )}

      <AlertDialog open={confirmRemove !== null} onOpenChange={(o) => !o && setConfirmRemove(null)}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>Remove {confirmRemove?.name}?</AlertDialogTitle>
            <AlertDialogDescription>
              This unregisters the environment from clio on this machine. The Creatio site itself,
              its data, and any installed packages are untouched — you can register it again at any
              time with the same URL and credentials.
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>Cancel</AlertDialogCancel>
            <AlertDialogAction
              className="bg-destructive text-destructive-foreground hover:bg-destructive/90"
              onClick={() => confirmRemove && remove(confirmRemove)}
            >
              Remove environment
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </div>
  );
}
