import { useState } from "react";
import { Button } from "@/components/ui/button";
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
import ErrorNote from "../../lib/ErrorNote";
import { logError } from "../../lib/errorLog";
import { EnvSummary, runClioJob } from "../../lib/ipc";

interface Props {
  env: EnvSummary;
  onClose: () => void;
  onSubmitted: (jobId: string) => void;
}

/// Update a registered environment's URL and credentials. clio's reg-web-app
/// updates an existing registration in place when the name matches, so this is
/// the same command the Add dialog uses.
///
/// The current password is deliberately never shown or pre-filled: clio owns the
/// credential store, and DevHub does not read secrets back out of it.
export default function EditEnvironmentDialog({ env, onClose, onSubmitted }: Props) {
  const [uri, setUri] = useState(env.uri);
  const [login, setLogin] = useState("");
  const [password, setPassword] = useState("");
  const [error, setError] = useState("");

  const submit = async () => {
    if (!uri.trim()) {
      setError("URL is required.");
      return;
    }
    if (!login.trim() || !password) {
      setError("Enter the login and the new password. Both are needed to update the credentials.");
      return;
    }
    const args = ["reg-web-app", env.name, "-u", uri.trim(), "-l", login.trim(), "-p", password];
    try {
      const jobId = await runClioJob("reg-web-app", args, env.name);
      onSubmitted(jobId);
    } catch (e) {
      const message = String(e);
      setError(message);
      logError("Environments", message, { context: "Update", env: env.name });
    }
  };

  return (
    <Dialog open onOpenChange={(o) => !o && onClose()}>
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle>Environment settings — {env.name}</DialogTitle>
          <DialogDescription>
            Credentials live in clio's own settings file. DevHub cannot read the current password,
            so changing it means re-entering both the login and the new password.
          </DialogDescription>
        </DialogHeader>
        <div className="grid gap-4">
          <div className="grid gap-2">
            <Label htmlFor="edit-uri">URL</Label>
            <Input id="edit-uri" value={uri} onChange={(e) => setUri(e.target.value)} />
          </div>
          <div className="grid gap-2">
            <Label htmlFor="edit-login">Login</Label>
            <Input
              id="edit-login"
              value={login}
              onChange={(e) => setLogin(e.target.value)}
              placeholder="Supervisor"
              autoFocus
            />
          </div>
          <div className="grid gap-2">
            <Label htmlFor="edit-password">New password</Label>
            <Input
              id="edit-password"
              type="password"
              value={password}
              onChange={(e) => setPassword(e.target.value)}
            />
          </div>
          {error && <ErrorNote error={error} />}
        </div>
        <DialogFooter>
          <Button variant="outline" onClick={onClose}>Cancel</Button>
          <Button onClick={submit}>Save changes</Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
