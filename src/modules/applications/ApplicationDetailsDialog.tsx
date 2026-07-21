import { Badge } from "@/components/ui/badge";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import ErrorNote from "../../lib/ErrorNote";
import { ApplicationDetails, ApplicationInfo } from "../../lib/ipc";
import { shortDate } from "./format";

/** One label/value pair; renders nothing when there is no value to show. */
function Fact({ label, value }: { label: string; value: string }) {
  if (!value) return null;
  return (
    <>
      <dt className="text-muted-foreground">{label}</dt>
      <dd className="break-words">{value}</dd>
    </>
  );
}

/**
 * Everything DevHub can learn about one installed application.
 *
 * The content arrives from two sources that fail independently — clio's own
 * descriptor (pages, schema prefix) and SQL against `SysInstalledApp` (developer,
 * dates, packages) — so the dialog renders whatever came back and lists what
 * did not as notes. An environment without cliogate still gets the clio half.
 */
export default function ApplicationDetailsDialog({
  application,
  details,
  error,
  onClose,
}: {
  application: ApplicationInfo | null;
  details: ApplicationDetails | null;
  error: string;
  onClose: () => void;
}) {
  return (
    <Dialog open={application !== null} onOpenChange={(open) => !open && onClose()}>
      <DialogContent className="max-h-[85vh] overflow-y-auto sm:max-w-2xl">
        {application && (
          <>
            <DialogHeader>
              <DialogTitle>{application.name || application.code}</DialogTitle>
              <DialogDescription>
                <code className="font-mono">{application.code}</code>
                {application.version && ` · version ${application.version}`}
              </DialogDescription>
            </DialogHeader>

            {error && <ErrorNote error={error} />}
            {!details && !error && (
              <p className="text-sm text-muted-foreground">Reading the application descriptor…</p>
            )}

            {details && (
              <div className="grid gap-5">
                {details.description && <p className="text-sm">{details.description}</p>}

                <dl className="grid grid-cols-[10rem_1fr] gap-x-4 gap-y-1.5 text-sm">
                  <Fact label="Developer" value={details.maintainer} />
                  <Fact label="Version" value={details.version} />
                  <Fact label="Requires Creatio" value={details.requiredPlatformVersion} />
                  <Fact label="Schema prefix" value={details.schemaNamePrefix} />
                  <Fact label="Created" value={shortDate(details.createdOn)} />
                  <Fact label="Modified" value={shortDate(details.modifiedOn)} />
                  <Fact label="Installed" value={shortDate(details.installDate)} />
                  <Fact label="Last update" value={shortDate(details.lastUpdate)} />
                  <Fact label="Marketplace" value={details.marketplaceLink} />
                  <Fact label="Help" value={details.helpLink} />
                  <Fact label="Support" value={details.supportEmail} />
                  <Fact label="Hidden" value={details.isHidden} />
                  <Fact label="Update available" value={details.needsUpdate} />
                </dl>

                <section className="grid gap-2">
                  <h3 className="flex items-center gap-2 text-sm font-semibold">
                    Packages
                    <Badge variant="secondary">{details.packages.length}</Badge>
                  </h3>
                  {details.packages.length === 0 ? (
                    <p className="text-sm text-muted-foreground">
                      No package list available for this application.
                    </p>
                  ) : (
                    <div className="overflow-x-auto rounded-lg border">
                      <Table>
                        <TableHeader>
                          <TableRow>
                            <TableHead>Package</TableHead>
                            <TableHead>Version</TableHead>
                            <TableHead>Maintainer</TableHead>
                          </TableRow>
                        </TableHeader>
                        <TableBody>
                          {details.packages.map((pkg) => (
                            <TableRow key={pkg.name}>
                              <TableCell className="font-medium">{pkg.name}</TableCell>
                              <TableCell>
                                <code className="font-mono text-xs">{pkg.version || "—"}</code>
                              </TableCell>
                              <TableCell>{pkg.maintainer || "—"}</TableCell>
                            </TableRow>
                          ))}
                        </TableBody>
                      </Table>
                    </div>
                  )}
                </section>

                {details.pages.length > 0 && (
                  <section className="grid gap-2">
                    <h3 className="flex items-center gap-2 text-sm font-semibold">
                      Pages
                      <Badge variant="secondary">{details.pages.length}</Badge>
                    </h3>
                    <ul className="grid gap-1 text-sm">
                      {details.pages.map((page) => (
                        <li key={page.schemaName} className="flex flex-wrap items-baseline gap-2">
                          <code className="font-mono text-xs">{page.schemaName}</code>
                          {page.parentSchemaName && (
                            <span className="text-xs text-muted-foreground">
                              extends {page.parentSchemaName}
                            </span>
                          )}
                        </li>
                      ))}
                    </ul>
                  </section>
                )}

                {details.notes.length > 0 && (
                  <section className="grid gap-1 rounded-md bg-muted p-3 text-xs text-muted-foreground">
                    {details.notes.map((note) => <p key={note}>{note}</p>)}
                  </section>
                )}
              </div>
            )}
          </>
        )}
      </DialogContent>
    </Dialog>
  );
}
