import { useState } from "react";
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
    <div className="dialog-backdrop" onClick={onClose}>
      <div className="dialog" onClick={(e) => e.stopPropagation()}>
        <h2>Add environment</h2>
        <label>
          Name
          <input value={name} onChange={(e) => setName(e.target.value)} placeholder="dev-834" autoFocus />
        </label>
        <label>
          URL
          <input value={uri} onChange={(e) => setUri(e.target.value)} placeholder="https://mysite.creatio.com" />
        </label>
        <div className="auth-toggle">
          <button className={authMode === "password" ? "on" : ""} onClick={() => setAuthMode("password")}>
            Login / password
          </button>
          <button className={authMode === "oauth" ? "on" : ""} onClick={() => setAuthMode("oauth")}>
            OAuth
          </button>
        </div>
        {authMode === "password" ? (
          <>
            <label>
              Login
              <input value={login} onChange={(e) => setLogin(e.target.value)} placeholder="Supervisor" />
            </label>
            <label>
              Password
              <input type="password" value={password} onChange={(e) => setPassword(e.target.value)} />
            </label>
          </>
        ) : (
          <>
            <label>
              Client ID
              <input value={clientId} onChange={(e) => setClientId(e.target.value)} />
            </label>
            <label>
              Client secret
              <input type="password" value={clientSecret} onChange={(e) => setClientSecret(e.target.value)} />
            </label>
            <label>
              Auth app URL
              <input
                value={authAppUri}
                onChange={(e) => setAuthAppUri(e.target.value)}
                placeholder="https://mysite-is.creatio.com/connect/token"
              />
            </label>
          </>
        )}
        {error && <ErrorNote error={error} />}
        <p className="hint">Credentials are stored by clio in its own settings file — this app keeps nothing.</p>
        <div className="dialog-actions">
          <button className="ghost" onClick={onClose}>
            Cancel
          </button>
          <button className="primary" onClick={submit}>
            Register
          </button>
        </div>
      </div>
    </div>
  );
}
