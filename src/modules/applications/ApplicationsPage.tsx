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
  ApplicationDetails, applicationDetails, ApplicationExtras, applicationExtras, ApplicationInfo,
  deployApplicationBetweenEnvironments, EnvSummary, listApplications, listEnvironments,
  onCatalogUpdated,
} from "../../lib/ipc";
import ApplicationDetailsDialog from "./ApplicationDetailsDialog";
import { shortDate } from "./format";

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
  /** Descriptor facts keyed by application code. Empty when SQL is unavailable. */
  const [extras, setExtras] = useState<Record<string, ApplicationExtras>>({});
  const [details, setDetails] = useState<ApplicationDetails | null>(null);
  const [detailsFor, setDetailsFor] = useState<ApplicationInfo | null>(null);
  const [detailsError, setDetailsError] = useState("");

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

  // Maintainer, dates and package counts come from SQL, which not every
  // environment allows. Purely additive: a failure leaves the tiles as clio
  // described them and is never surfaced as an error.
  useEffect(() => {
    setExtras({});
    if (!sourceEnv) return;
    let current = true;
    applicationExtras(sourceEnv)
      .then((rows) => {
        if (!current) return;
        setExtras(Object.fromEntries(rows.map((row) => [row.code, row])));
      })
      .catch(() => { /* no SQL access here — tiles stay as they were */ });
    return () => { current = false; };
  }, [sourceEnv]);

  const openDetails = async (application: ApplicationInfo) => {
    setDetailsFor(application);
    setDetails(null);
    setDetailsError("");
    try {
      setDetails(await applicationDetails(sourceEnv, application.code));
    } catch (reason) {
      setDetailsError(String(reason));
    }
  };

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
      application.description?.toLowerCase().includes(query) ||
      // Only searchable once the descriptor read succeeded — "Creatio" vs
      // "Customer" is the split people actually want here.
      extras[application.code]?.maintainer.toLowerCase().includes(query));
  }, [applications, extras, filter]);

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
            placeholder="Name, code, version, or developer"
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
            // h-full so every card fills its grid row: without it a card with
            // fewer facts is shorter and its buttons sit above its neighbours'.
            <Card key={application.id || application.code} className="h-full">
              <CardHeader>
                <CardTitle className="text-base">{application.name || application.code}</CardTitle>
                <code className="font-mono text-xs text-muted-foreground">{application.code}</code>
                <CardAction>
                  <Badge className="border-transparent bg-accent/15 text-accent-foreground">
                    {application.version || "no version"}
                  </Badge>
                </CardAction>
              </CardHeader>
              {/* flex-1 pushes the footer to the bottom of every card, so the
                  action buttons form one line across the row. */}
              <CardContent className="flex flex-1 flex-col gap-3 text-sm text-muted-foreground">
                <p>{application.description || "No application description."}</p>
                {extras[application.code] && (
                  // Fixed label column and a full set of rows — an app missing a
                  // value shows an em dash rather than collapsing the row, which
                  // would misalign it against the cards beside it.
                  <dl className="mt-auto grid grid-cols-[7rem_1fr] gap-x-3 gap-y-1 text-xs">
                    <dt className="text-muted-foreground/80">Developer</dt>
                    <dd className="text-foreground">
                      {extras[application.code].maintainer || "—"}
                    </dd>
                    <dt className="text-muted-foreground/80">Packages</dt>
                    <dd className="text-foreground">{extras[application.code].packageCount}</dd>
                    <dt className="text-muted-foreground/80">Needs Creatio</dt>
                    <dd className="text-foreground">
                      {extras[application.code].requiredPlatformVersion || "—"}
                    </dd>
                    <dt className="text-muted-foreground/80">Updated</dt>
                    <dd className="text-foreground">
                      {shortDate(extras[application.code].modifiedOn) || "—"}
                    </dd>
                  </dl>
                )}
              </CardContent>
              {/* Both actions share the footer width, so the buttons line up
                  edge to edge across every card instead of ending wherever
                  their own label happens to stop. */}
              <CardFooter className="gap-2">
                <Button size="sm" className="flex-1" onClick={() => chooseTarget(application)}>
                  Deploy to environment…
                </Button>
                <Button
                  size="sm"
                  variant="outline"
                  className="flex-1"
                  onClick={() => openDetails(application)}
                >
                  Details
                </Button>
              </CardFooter>
            </Card>
          ))}
        </div>
      )}

      <ApplicationDetailsDialog
        application={detailsFor}
        details={details}
        error={detailsError}
        onClose={() => { setDetailsFor(null); setDetails(null); setDetailsError(""); }}
      />

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
