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
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs";
import ErrorNote from "../../lib/ErrorNote";
import { runClioJob } from "../../lib/ipc";

interface Props {
  onClose: () => void;
  onSubmitted: (jobId: string) => void;
}

export default function AddEnvironmentDialog({ onClose, onSubmitted }: Props) {
  const [name, setName] = useState("");
  const [uri, setUri] = useState("");
  const [authMode, setAuthMode] = useState<"password" | "oauth">("password");
  const [login, setLogin] = useState("");
  const [password, setPassword] = useState("");
  const [clientId, setClientId] = useState("");
  const [clientSecret, setClientSecret] = useState("");
  const [authAppUri, setAuthAppUri] = useState("");
  const [error, setError] = useState("");

  const submit = async () => {
    if (!name.trim() || !uri.trim()) {
      setError("Name and URL are required.");
      return;
    }
    const args = ["reg-web-app", name.trim(), "-u", uri.trim()];
    if (authMode === "password") {
      if (!login || !password) {
        setError("Login and password are required.");
        return;
      }
      args.push("-l", login, "-p", password);
    } else {
      if (!clientId || !clientSecret || !authAppUri) {
        setError("ClientId, ClientSecret and Auth App URL are required for OAuth.");
        return;
      }
      args.push("--ClientId", clientId, "--ClientSecret", clientSecret, "--AuthAppUri", authAppUri);
    }
    const jobId = await runClioJob("reg-web-app", args, name.trim());
    onSubmitted(jobId);
  };

  return (
    <Dialog open onOpenChange={(o) => !o && onClose()}>
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle>Add environment</DialogTitle>
          <DialogDescription>
            Credentials are stored by clio in its own settings file — this app keeps nothing.
          </DialogDescription>
        </DialogHeader>
        <div className="grid gap-4">
          <div className="grid gap-2">
            <Label htmlFor="env-name">Name</Label>
            <Input
              id="env-name"
              value={name}
              onChange={(e) => setName(e.target.value)}
              placeholder="dev-834"
              autoFocus
            />
          </div>
          <div className="grid gap-2">
            <Label htmlFor="env-uri">URL</Label>
            <Input
              id="env-uri"
              value={uri}
              onChange={(e) => setUri(e.target.value)}
              placeholder="https://mysite.creatio.com"
            />
          </div>
          <Tabs value={authMode} onValueChange={(v) => setAuthMode(v as "password" | "oauth")}>
            <TabsList className="grid w-full grid-cols-2">
              <TabsTrigger value="password">Login / password</TabsTrigger>
              <TabsTrigger value="oauth">OAuth</TabsTrigger>
            </TabsList>
          </Tabs>
          {authMode === "password" ? (
            <>
              <div className="grid gap-2">
                <Label htmlFor="env-login">Login</Label>
                <Input
                  id="env-login"
                  value={login}
                  onChange={(e) => setLogin(e.target.value)}
                  placeholder="Supervisor"
                />
              </div>
              <div className="grid gap-2">
                <Label htmlFor="env-password">Password</Label>
                <Input
                  id="env-password"
                  type="password"
                  value={password}
                  onChange={(e) => setPassword(e.target.value)}
                />
              </div>
            </>
          ) : (
            <>
              <div className="grid gap-2">
                <Label htmlFor="env-client-id">Client ID</Label>
                <Input id="env-client-id" value={clientId} onChange={(e) => setClientId(e.target.value)} />
              </div>
              <div className="grid gap-2">
                <Label htmlFor="env-client-secret">Client secret</Label>
                <Input
                  id="env-client-secret"
                  type="password"
                  value={clientSecret}
                  onChange={(e) => setClientSecret(e.target.value)}
                />
              </div>
              <div className="grid gap-2">
                <Label htmlFor="env-auth-uri">Auth app URL</Label>
                <Input
                  id="env-auth-uri"
                  value={authAppUri}
                  onChange={(e) => setAuthAppUri(e.target.value)}
                  placeholder="https://mysite-is.creatio.com/connect/token"
                />
              </div>
            </>
          )}
          {error && <ErrorNote error={error} />}
        </div>
        <DialogFooter>
          <Button variant="outline" onClick={onClose}>Cancel</Button>
          <Button onClick={submit}>Register</Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
