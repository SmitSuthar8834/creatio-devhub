import { useEffect, useState } from "react";
import {
  Blocks,
  Database,
  FolderGit2,
  ListChecks,
  Package,
  Server,
  Settings,
} from "lucide-react";
import {
  Sidebar,
  SidebarContent,
  SidebarFooter,
  SidebarGroup,
  SidebarGroupContent,
  SidebarGroupLabel,
  SidebarHeader,
  SidebarInset,
  SidebarMenu,
  SidebarMenuButton,
  SidebarMenuItem,
  SidebarProvider,
  SidebarTrigger,
} from "@/components/ui/sidebar";
import { Separator } from "@/components/ui/separator";
import { Button } from "@/components/ui/button";
import { Toaster } from "@/components/ui/sonner";
import { listEnvironments, onEnvironmentChanged, prefetchEnvCatalog } from "./lib/ipc";
import ApplicationsPage from "./modules/applications/ApplicationsPage";
import ClioBanner from "./modules/clio/ClioBanner";
import EnvironmentsPage from "./modules/environments/EnvironmentsPage";
import JobsPage from "./modules/jobs/JobsPage";
import JobToaster from "./modules/jobs/JobToaster";
import PackagesPage from "./modules/packages/PackagesPage";
import SettingsPage from "./modules/settings/SettingsPage";
import SqlPage from "./modules/sql/SqlPage";
import WorkspacesPage from "./modules/workspaces/WorkspacesPage";
import logoMark from "./assets/icons/logo-mark.png";

type Page = "environments" | "workspaces" | "packages" | "applications" | "sql" | "jobs" | "settings";

const NAV: { id: Page; label: string; icon: typeof Server }[] = [
  { id: "environments", label: "Environments", icon: Server },
  { id: "workspaces", label: "Workspaces", icon: FolderGit2 },
  { id: "packages", label: "Packages", icon: Package },
  { id: "applications", label: "Applications", icon: Blocks },
  { id: "sql", label: "SQL", icon: Database },
  { id: "jobs", label: "Jobs", icon: ListChecks },
  { id: "settings", label: "Settings", icon: Settings },
];

export default function App() {
  const [page, setPage] = useState<Page>("environments");
  const [workspaceToOpen, setWorkspaceToOpen] = useState<string | null>(null);

  // Auto-capture catalog state: warm the active environment's cache on launch,
  // and again whenever the default environment changes.
  useEffect(() => {
    listEnvironments()
      .then((list) => {
        const active = list.find((e) => e.isActive) ?? list[0];
        if (active) prefetchEnvCatalog(active.name);
      })
      .catch(() => {});
    const un = onEnvironmentChanged((env) => prefetchEnvCatalog(env));
    return () => {
      un.then((f) => f());
    };
  }, []);

  return (
    <SidebarProvider>
      <Sidebar collapsible="icon">
        <SidebarHeader>
          <div className="flex items-center gap-2.5 px-1 py-1.5">
            <img
              src={logoMark}
              alt=""
              className="size-8 shrink-0 rounded-lg border bg-white object-contain p-0.5"
            />
            <div className="grid leading-tight group-data-[collapsible=icon]:hidden">
              <span className="text-sm font-semibold">DevHub</span>
              <span className="mt-0.5 text-[10px] leading-snug text-balance text-sidebar-foreground/70">
                Creatio development, version control and deployment—simplified.
              </span>
            </div>
          </div>
        </SidebarHeader>
        <SidebarContent>
          <SidebarGroup>
            <SidebarGroupLabel>Workspace</SidebarGroupLabel>
            <SidebarGroupContent>
              <SidebarMenu>
                {NAV.map((n) => (
                  <SidebarMenuItem key={n.id}>
                    <SidebarMenuButton
                      isActive={page === n.id}
                      tooltip={n.label}
                      onClick={() => {
                        if (n.id === "workspaces") setWorkspaceToOpen(null);
                        setPage(n.id);
                      }}
                    >
                      <n.icon />
                      <span>{n.label}</span>
                    </SidebarMenuButton>
                  </SidebarMenuItem>
                ))}
              </SidebarMenu>
            </SidebarGroupContent>
          </SidebarGroup>
        </SidebarContent>
        <SidebarFooter>
          <div className="flex items-center gap-2 px-2 py-1 text-xs text-sidebar-foreground/70 group-data-[collapsible=icon]:hidden">
            <span className="size-1.5 rounded-full bg-success" aria-hidden="true" />
            Local desktop
          </div>
        </SidebarFooter>
      </Sidebar>
      <SidebarInset className="flex h-svh flex-col overflow-hidden">
        <header className="flex h-14 shrink-0 items-center gap-2 border-b px-4">
          <SidebarTrigger className="-ml-1" />
          <Separator orientation="vertical" className="mr-1 !h-4" />
          <div className="text-sm">
            <span className="font-semibold">{NAV.find((item) => item.id === page)?.label}</span>
            <span className="text-muted-foreground"> / DevHub</span>
          </div>
          <Button
            variant="ghost"
            size="sm"
            className="ml-auto text-primary"
            onClick={() => setPage("settings")}
          >
            Help &amp; updates
          </Button>
        </header>
        <ClioBanner onShowJobs={() => setPage("jobs")} />
        <div className="flex-1 overflow-y-auto">
          {page === "environments" && <EnvironmentsPage />}
          {page === "jobs" && <JobsPage />}
          {page === "workspaces" && <WorkspacesPage
            onShowJobs={() => setPage("jobs")}
            initialWorkspaceId={workspaceToOpen}
            onWorkspaceClosed={() => setWorkspaceToOpen(null)}
          />}
          {page === "packages" && <PackagesPage
            onShowJobs={() => setPage("jobs")}
            onOpenWorkspace={(id) => {
              setWorkspaceToOpen(id);
              setPage("workspaces");
            }}
          />}
          {page === "applications" && <ApplicationsPage onShowJobs={() => setPage("jobs")} />}
          {page === "sql" && <SqlPage onShowJobs={() => setPage("jobs")} />}
          {page === "settings" && <SettingsPage />}
        </div>
      </SidebarInset>
      <JobToaster onShowJobs={() => setPage("jobs")} />
      <Toaster position="bottom-right" closeButton />
    </SidebarProvider>
  );
}
