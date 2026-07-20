import { useEffect, useState } from "react";
import { Diagnosis, diagnoseError } from "./ipc";

/// Renders an error with its cause and resolution steps when DevHub recognizes
/// it, and the raw message alone when it does not. Use this anywhere a raw
/// clio/git/gh failure would otherwise be shown to the user.
export default function ErrorNote({ error }: { error: string }) {
  const [diagnosis, setDiagnosis] = useState<Diagnosis | null>(null);

  useEffect(() => {
    let current = true;
    setDiagnosis(null);
    if (error) {
      diagnoseError(error)
        .then((found) => { if (current) setDiagnosis(found); })
        .catch(() => { /* diagnosis is an enhancement; the raw error still shows */ });
    }
    return () => { current = false; };
  }, [error]);

  if (!error) return null;
  if (!diagnosis) return <p className="form-error">{error}</p>;

  return (
    <section className="error-note" aria-label="Error and resolution">
      <p className="form-error">{diagnosis.summary}</p>
      <p className="hint">{diagnosis.cause}</p>
      <h4>How to fix it</h4>
      <ol>
        {diagnosis.steps.map((step) => <li key={step}>{step}</li>)}
      </ol>
      <details>
        <summary>Technical detail</summary>
        <pre>{error}</pre>
      </details>
    </section>
  );
}
