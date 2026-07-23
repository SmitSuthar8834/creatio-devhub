import { useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { RadioGroup, RadioGroupItem } from "@/components/ui/radio-group";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs";
import ErrorNote from "../../lib/ErrorNote";
import { logError } from "../../lib/errorLog";
import { createWorkspaceFlow, EnvSummary, listEnvironments, registerWorkspace } from "../../lib/ipc";

interface Props {
  onClose: () => void;
  onStarted: () => void;
}

export default function NewWorkspaceWizard({ onClose, onStarted }: Props) {
  const [mode, setMode] = useState<"create" | "existing">("create");
  const [envs, setEnvs] = useState<EnvSummary[]>([]);
  const [env, setEnv] = useState("");
  const [name, setName] = useState("");
  const [parentDir, setParentDir] = useState("");
  const [appCode, setAppCode] = useState("");
  const [remoteUrl, setRemoteUrl] = useState("");
  const [startEmpty, setStartEmpty] = useState(true);
  const [existingPath, setExistingPath] = useState("");
  const [error, setError] = useState("");
  // Show the failure inline and record it into the app-wide Errors view.
  const reportError = (e: unknown) => {
    const message = String(e);
    setError(message);
    logError("Workspaces", message);
  };
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    listEnvironments().then((list) => {
      setEnvs(list);
      const active = list.find((e) => e.isActive) ?? list[0];
      if (active) setEnv(active.name);
    });
  }, []);

  const pickFolder = async (set: (v: string) => void) => {
    const dir = await open({ directory: true });
    if (typeof dir === "string") set(dir);
  };

  const submit = async () => {
    setError("");
    setBusy(true);
    try {
      if (mode === "create") {
        if (!name.trim() || !parentDir.trim() || !env) {
          setError("Name, folder and environment are required.");
          return;
        }
        await createWorkspaceFlow({
          name: name.trim(),
          parentDir: parentDir.trim(),
          env,
          appCode: appCode.trim() || undefined,
          remoteUrl: remoteUrl.trim() || undefined,
          skipRestore: startEmpty,
        });
      } else {
        if (!existingPath.trim() || !env) {
          setError("Folder and environment are required.");
          return;
        }
        await registerWorkspace(existingPath.trim(), env);
      }
      onStarted();
    } catch (e) {
      reportError(e);
    } finally {
      setBusy(false);
    }
  };

  return (
    <Dialog open onOpenChange={(o) => !o && onClose()}>
      <DialogContent className="sm:max-w-lg">
        <DialogHeader>
          <DialogTitle>New workspace</DialogTitle>
        </DialogHeader>

        <Tabs value={mode} onValueChange={(v) => setMode(v as "create" | "existing")}>
          <TabsList className="grid w-full grid-cols-2">
            <TabsTrigger value="create">Pull from environment</TabsTrigger>
            <TabsTrigger value="existing">Open existing folder</TabsTrigger>
          </TabsList>
        </Tabs>

        <div className="grid gap-4">
          <div className="grid gap-2">
            <Label htmlFor="ws-env">Environment</Label>
            <Select value={env} onValueChange={setEnv}>
              <SelectTrigger id="ws-env" className="w-full">
                <SelectValue placeholder="Select an environment" />
              </SelectTrigger>
              <SelectContent>
                {envs.map((e) => (
                  <SelectItem key={e.name} value={e.name}>
                    {e.name} {e.isActive ? "(default)" : ""}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </div>

          {mode === "create" ? (
            <>
              <div className="grid gap-2">
                <Label htmlFor="ws-name">Workspace name</Label>
                <Input
                  id="ws-name"
                  value={name}
                  onChange={(e) => setName(e.target.value)}
                  placeholder="my-app-workspace"
                  autoFocus
                />
              </div>
              <div className="grid gap-2">
                <Label htmlFor="ws-parent">Create inside folder</Label>
                <div className="flex gap-2">
                  <Input
                    id="ws-parent"
                    value={parentDir}
                    onChange={(e) => setParentDir(e.target.value)}
                    placeholder="A:\CreatioWorkspaces"
                  />
                  <Button variant="outline" onClick={() => pickFolder(setParentDir)}>Browse…</Button>
                </div>
              </div>
              <div className="grid gap-2">
                <Label htmlFor="ws-appcode">
                  App code{" "}
                  <span className="text-muted-foreground">
                    (optional — limits the workspace to one app)
                  </span>
                </Label>
                <Input
                  id="ws-appcode"
                  value={appCode}
                  onChange={(e) => setAppCode(e.target.value)}
                  placeholder="MyAppCode"
                />
              </div>

              <div className="grid gap-2">
                <Label>Contents</Label>
                <RadioGroup
                  value={startEmpty ? "empty" : "all"}
                  onValueChange={(v) => setStartEmpty(v === "empty")}
                  className="grid gap-2"
                >
                  <Label
                    htmlFor="ws-empty"
                    className="flex cursor-pointer items-start gap-3 rounded-lg border p-3 font-normal has-[[data-state=checked]]:border-primary has-[[data-state=checked]]:bg-accent/10"
                  >
                    <RadioGroupItem value="empty" id="ws-empty" className="mt-0.5" />
                    <span className="text-sm">
                      <strong>Start empty</strong> — scaffold only, pick packages to add afterwards{" "}
                      <span className="text-muted-foreground">(recommended)</span>
                    </span>
                  </Label>
                  <Label
                    htmlFor="ws-all"
                    className="flex cursor-pointer items-start gap-3 rounded-lg border p-3 font-normal has-[[data-state=checked]]:border-primary has-[[data-state=checked]]:bg-accent/10"
                  >
                    <RadioGroupItem value="all" id="ws-all" className="mt-0.5" />
                    <span className="text-sm">
                      <strong>Pull everything now</strong> — download all editable packages from the
                      environment
                    </span>
                  </Label>
                </RadioGroup>
              </div>

              <div className="grid gap-2">
                <Label htmlFor="ws-remote">
                  Git remote URL{" "}
                  <span className="text-muted-foreground">
                    (optional — you can also create a GitHub repo later from the workspace)
                  </span>
                </Label>
                <Input
                  id="ws-remote"
                  value={remoteUrl}
                  onChange={(e) => setRemoteUrl(e.target.value)}
                  placeholder="https://github.com/you/repo.git"
                />
              </div>
              <p className="text-sm text-muted-foreground">
                {startEmpty
                  ? "Creates an empty clio workspace and an initial git commit — no packages downloaded. Add packages and create a GitHub repo from the workspace screen. Watch progress on the Jobs screen."
                  : "Runs create-workspace + restore-workspace against the environment, then initializes git with an initial commit. Watch progress on the Jobs screen."}
              </p>
            </>
          ) : (
            <>
              <div className="grid gap-2">
                <Label htmlFor="ws-existing">Workspace folder</Label>
                <div className="flex gap-2">
                  <Input
                    id="ws-existing"
                    value={existingPath}
                    onChange={(e) => setExistingPath(e.target.value)}
                    placeholder="C:\path\to\cloned-workspace"
                  />
                  <Button variant="outline" onClick={() => pickFolder(setExistingPath)}>Browse…</Button>
                </div>
              </div>
              <p className="text-sm text-muted-foreground">
                For a cloned repo or a workspace created outside DevHub. The folder must contain
                .clio.
              </p>
            </>
          )}

          {error && <ErrorNote error={error} />}
        </div>

        <DialogFooter>
          <Button variant="outline" onClick={onClose}>Cancel</Button>
          <Button onClick={submit} disabled={busy}>
            {mode === "create" ? "Create workspace" : "Add workspace"}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
