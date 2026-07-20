import { useEffect, useState } from "react";
import { ChevronRight, CircleAlert } from "lucide-react";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "@/components/ui/collapsible";
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

  if (!diagnosis) {
    return (
      <Alert variant="destructive">
        <CircleAlert aria-hidden="true" />
        <AlertDescription className="break-words">{error}</AlertDescription>
      </Alert>
    );
  }

  return (
    <Alert variant="destructive">
      <CircleAlert aria-hidden="true" />
      <AlertTitle>{diagnosis.summary}</AlertTitle>
      <AlertDescription>
        <p>{diagnosis.cause}</p>
        <p className="mt-2 font-medium text-foreground">How to fix it</p>
        <ol className="mt-1 list-decimal space-y-1 pl-5">
          {diagnosis.steps.map((step) => <li key={step}>{step}</li>)}
        </ol>
        <Collapsible className="mt-2">
          <CollapsibleTrigger className="group flex items-center gap-1 text-xs font-medium">
            <ChevronRight
              className="size-3 transition-transform group-data-[state=open]:rotate-90"
              aria-hidden="true"
            />
            Technical detail
          </CollapsibleTrigger>
          <CollapsibleContent>
            <pre className="mt-2 max-h-44 overflow-auto whitespace-pre-wrap break-words rounded-md bg-muted p-2 font-mono text-xs text-foreground">
              {error}
            </pre>
          </CollapsibleContent>
        </Collapsible>
      </AlertDescription>
    </Alert>
  );
}
