import { useEffect, useMemo, useState } from "react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  Card,
  CardAction,
  CardContent,
  CardFooter,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import ErrorNote from "../../lib/ErrorNote";
import {
  ApplicationInfo, deployApplicationBetweenEnvironments, EnvSummary,
  listApplications, listEnvironments, onCatalogUpdated,
} from "../../lib/ipc";

export default function ApplicationsPage({ onShowJobs }: { onShowJobs: () => void }) {
  const [environments, setEnvironments] = useState<EnvSummary[]>([]);
  const [sourceEnv, setSourceEnv] = useState("");
  const [applications, setApplications] = useState<ApplicationInfo[]>([]);
  const [filter, setFilter] = useState("");
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState("");
  const [notice, setNotice] = useState("");
  const [selectedApp, setSelectedApp] = useState<ApplicationInfo | null>(null);
  const [targetEnv, setTargetEnv] = useState("");
  const [confirmation, setConfirmation] = useState("");
  const [cachedAt, setCachedAt] = useState<number | null>(null);
  const [fromCache, setFromCache] = useState(false);

  useEffect(() => {
    listEnvironments().then((list) => {
      setEnvironments(list);
      const initial = list.find((environment) => environment.isActive) ?? list[0];
      if (initial) setSourceEnv(initial.name);
    }).catch((reason) => setError(String(reason)));
  }, []);

  const refresh = async (forceRefresh = true) => {
    if (!sourceEnv) return;
    setLoading(true);
    setError("");
    try {
      const result = await listApplications(sourceEnv, forceRefresh);
      setApplications(result.items);
      setCachedAt(result.cachedAt);
      setFromCache(result.fromCache);
    } catch (reason) {
      setApplications([]);
      setError(String(reason));
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => { refresh(false); }, [sourceEnv]);

  // When the background prefetch freshens this environment's cache, reload from it.
  useEffect(() => {
    const un = onCatalogUpdated((env) => { if (env === sourceEnv) refresh(false); });
    return () => { un.then((f) => f()); };
  }, [sourceEnv]);

  const visible = useMemo(() => {
    const query = filter.trim().toLowerCase();
    if (!query) return applications;
    return applications.filter((application) =>
      application.name.toLowerCase().includes(query) ||
      application.code.toLowerCase().includes(query) ||
      application.version.toLowerCase().includes(query) ||
      application.description?.toLowerCase().includes(query));
  }, [applications, filter]);

  const chooseTarget = (application: ApplicationInfo) => {
    const target = environments.find((environment) => environment.name !== sourceEnv)?.name ?? "";
    setSelectedApp(application);
    setTargetEnv(target);
    setConfirmation("");
    setError("");
  };

  const closeDialog = () => {
    setSelectedApp(null);
    setTargetEnv("");
    setConfirmation("");
  };

  const deploy = async () => {
    if (!selectedApp || !targetEnv) return;
    const application = selectedApp;
    const target = targetEnv;
    closeDialog();
    setError("");
    setNotice("");
    try {
      await deployApplicationBetweenEnvironments({
        sourceEnv,
        targetEnv: target,
        appCode: application.code,
      });
      setNotice(`Deploying ${application.name} from ${sourceEnv} to ${target}. Follow the streamed output in Jobs.`);
    } catch (reason) {
      setError(String(reason));
    }
  };

  return (
    <div className="mx-auto grid max-w-6xl gap-4 p-6">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <h1 className="text-xl font-semibold tracking-tight">Applications</h1>
        <div className="flex flex-wrap gap-2">
          <Button variant="ghost" onClick={onShowJobs}>Jobs</Button>
          <Button onClick={() => refresh(true)} disabled={!sourceEnv || loading}>
            {loading ? "Loading…" : "Refresh"}
          </Button>
        </div>
      </div>

      <div className="flex flex-wrap items-end gap-3">
        <div className="grid min-w-56 gap-2">
          <Label htmlFor="app-env">Source environment</Label>
          <Select value={sourceEnv} onValueChange={setSourceEnv}>
            <SelectTrigger id="app-env" className="w-full">
              <SelectValue placeholder="Select an environment" />
            </SelectTrigger>
            <SelectContent>
              {environments.map((environment) => (
                <SelectItem key={environment.name} value={environment.name}>
                  {environment.name} {environment.isActive ? "(default)" : ""}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </div>
        <div className="grid min-w-64 flex-1 gap-2">
          <Label htmlFor="app-filter">Filter</Label>
          <Input
            id="app-filter"
            value={filter}
            onChange={(event) => setFilter(event.target.value)}
            placeholder="Application name, code, or version"
          />
        </div>
        <span className="pb-2.5 text-sm text-muted-foreground">
          {visible.length} of {applications.length}
        </span>
      </div>

      <p className="text-sm text-muted-foreground">
        Application deployment transfers the complete Creatio application represented by its
        application descriptor, including its application packages. It is different from deploying
        one package from the Packages screen.
      </p>
      {cachedAt && (
        <p className="text-xs text-muted-foreground">
          {fromCache ? "Showing saved data" : "Updated"} from {new Date(cachedAt).toLocaleString()}.
          {fromCache && " Use Refresh to check the environment for changes."}
        </p>
      )}
      {notice && <p className="text-sm text-muted-foreground">{notice}</p>}
      {error && <ErrorNote error={error} />}

      {!loading && applications.length === 0 && !error ? (
        <p className="text-muted-foreground">
          No installed applications were returned for this environment.
        </p>
      ) : (
        <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
          {visible.map((application) => (
            <Card key={application.id || application.code}>
              <CardHeader>
                <CardTitle className="text-base">{application.name || application.code}</CardTitle>
                <code className="font-mono text-xs text-muted-foreground">{application.code}</code>
                <CardAction>
                  <Badge className="border-transparent bg-accent/15 text-accent-foreground">
                    {application.version || "no version"}
                  </Badge>
                </CardAction>
              </CardHeader>
              <CardContent className="text-sm text-muted-foreground">
                {application.description || "No application description."}
              </CardContent>
              <CardFooter>
                <Button size="sm" onClick={() => chooseTarget(application)}>
                  Deploy to environment…
                </Button>
              </CardFooter>
            </Card>
          ))}
        </div>
      )}

      <Dialog open={selectedApp !== null} onOpenChange={(o) => !o && closeDialog()}>
        <DialogContent>
          {selectedApp && (
            <>
              <DialogHeader>
                <DialogTitle>Deploy {selectedApp.name || selectedApp.code}</DialogTitle>
                <DialogDescription>
                  Application <strong>{selectedApp.code}</strong> version{" "}
                  <strong>{selectedApp.version || "unspecified"}</strong> will be transferred from{" "}
                  <strong>{sourceEnv}</strong> and installed into the target.
                </DialogDescription>
              </DialogHeader>
              <p className="text-sm text-destructive">
                This can update multiple packages and start target-side installation or compilation.
                It cannot be safely cancelled after deployment begins.
              </p>
              <div className="grid gap-2">
                <Label htmlFor="app-target">Target environment</Label>
                <Select
                  value={targetEnv}
                  onValueChange={(value) => { setTargetEnv(value); setConfirmation(""); }}
                >
                  <SelectTrigger id="app-target" className="w-full">
                    <SelectValue placeholder="Select a target" />
                  </SelectTrigger>
                  <SelectContent>
                    {environments.filter((environment) => environment.name !== sourceEnv).map((environment) => (
                      <SelectItem key={environment.name} value={environment.name}>
                        {environment.name} — {environment.uri}
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
              </div>
              <div className="grid gap-2">
                <Label htmlFor="app-confirm">
                  Type <strong>{targetEnv || "the target environment"}</strong> to confirm
                </Label>
                <Input
                  id="app-confirm"
                  value={confirmation}
                  onChange={(event) => setConfirmation(event.target.value)}
                  autoFocus
                />
              </div>
              <DialogFooter>
                <Button variant="outline" onClick={closeDialog}>Cancel</Button>
                <Button
                  variant="destructive"
                  disabled={!targetEnv || confirmation !== targetEnv}
                  onClick={deploy}
                >
                  Deploy application
                </Button>
              </DialogFooter>
            </>
          )}
        </DialogContent>
      </Dialog>
    </div>
  );
}
