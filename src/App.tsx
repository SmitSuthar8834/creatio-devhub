import { useState } from "react";
import "./App.css";
import ApplicationsPage from "./modules/applications/ApplicationsPage";
import EnvironmentsPage from "./modules/environments/EnvironmentsPage";
import JobsPage from "./modules/jobs/JobsPage";
import PackagesPage from "./modules/packages/PackagesPage";
import SettingsPage from "./modules/settings/SettingsPage";
import WorkspacesPage from "./modules/workspaces/WorkspacesPage";
import logoMark from "./assets/icons/logo-mark.png";
import iconEnvironments from "./assets/icons/environments.png";
import iconWorkspaces from "./assets/icons/workspaces.png";
import iconPackages from "./assets/icons/packages.png";
import iconApplications from "./assets/icons/applications.png";
import iconJobs from "./assets/icons/jobs.png";
import iconSettings from "./assets/icons/settings.png";
import iconLocalDesktop from "./assets/icons/local-desktop.png";

type Page = "environments" | "workspaces" | "packages" | "applications" | "jobs" | "settings";

const NAV: { id: Page; label: string; icon: string }[] = [
  { id: "environments", label: "Environments", icon: iconEnvironments },
  { id: "workspaces", label: "Workspaces", icon: iconWorkspaces },
  { id: "packages", label: "Packages", icon: iconPackages },
  { id: "applications", label: "Applications", icon: iconApplications },
  { id: "jobs", label: "Jobs", icon: iconJobs },
  { id: "settings", label: "Settings", icon: iconSettings },
];

export default function App() {
  const [page, setPage] = useState<Page>("environments");
  const [workspaceToOpen, setWorkspaceToOpen] = useState<string | null>(null);

  return (
    <div className="shell">
      <nav className="sidebar">
        <div className="brand">
          <img className="brand-mark" src={logoMark} alt="Creatio DevHub" />
          <span className="brand-copy">DevHub<small>Creatio engineering</small></span>
        </div>
        <div className="nav-label">Workspace</div>
        {NAV.map((n) => (
          <button
            key={n.id}
            className={`nav-item ${page === n.id ? "on" : ""}`}
            onClick={() => {
              if (n.id === "workspaces") setWorkspaceToOpen(null);
              setPage(n.id);
            }}
          >
            <img className="nav-icon" src={n.icon} alt="" />{n.label}
          </button>
        ))}
        <div className="sidebar-footer">
          <img className="footer-icon" src={iconLocalDesktop} alt="" />
          <span className="status-light" /> Local desktop
        </div>
      </nav>
      <main className="content">
        <header className="topbar">
          <div><strong>{NAV.find((item) => item.id === page)?.label}</strong><span> / DevHub</span></div>
          <button className="topbar-help" onClick={() => setPage("settings")}>Help &amp; updates</button>
        </header>
        <div className="content-scroll">
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
        {page === "settings" && <SettingsPage />}
        </div>
      </main>
    </div>
  );
}
