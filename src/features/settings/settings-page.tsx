import { NavLink, Navigate, useParams } from "react-router-dom";
import { Page } from "@/components/app/page";
import { cn } from "@/lib/utils";
import { useSession } from "@/stores/session";
import { AuditSection } from "./audit-page";
import { BackupSection, DevicesSection } from "./devices-backup";
import { SECTIONS, SettingsSection } from "./settings-forms";
import { RolesSection, UsersSection } from "./users-roles";

interface Tab {
  id: string;
  title: string;
  permission: string;
}

const TABS: Tab[] = [
  { id: "business", title: "Business", permission: "settings.view" },
  { id: "tax", title: "Tax and pricing", permission: "settings.view" },
  { id: "invoice", title: "Invoices and printing", permission: "settings.view" },
  { id: "inventory", title: "Inventory", permission: "settings.view" },
  { id: "users", title: "Users and roles", permission: "users.manage" },
  { id: "security", title: "Security", permission: "settings.view" },
  { id: "network", title: "Network and counters", permission: "settings.view" },
  { id: "backup", title: "Backup", permission: "backup.manage" },
  { id: "audit", title: "Audit log", permission: "audit.view" },
];

function Content({ id }: { id: string }) {
  switch (id) {
    case "users":
      return (
        <>
          <UsersSection />
          <RolesSection />
        </>
      );
    case "network":
      return (
        <>
          <SettingsSection name="network" />
          <DevicesSection />
        </>
      );
    case "backup":
      return (
        <>
          <SettingsSection name="backup" />
          <BackupSection />
        </>
      );
    case "audit":
      return <AuditSection />;
    default:
      return id in SECTIONS ? <SettingsSection name={id} /> : null;
  }
}

export default function SettingsPage() {
  const { section } = useParams();
  const permissions = useSession((s) => s.user?.permissions ?? []);
  const tabs = TABS.filter((t) => permissions.includes(t.permission));
  const first = tabs[0];
  if (!first) {
    return (
      <Page title="Settings">
        <p className="text-sm text-muted-foreground">Your role can't open settings.</p>
      </Page>
    );
  }
  if (!section || !tabs.some((t) => t.id === section))
    return <Navigate to={`/settings/${first.id}`} replace />;
  return (
    <Page title="Settings">
      <div className="grid gap-6 lg:grid-cols-[14rem_1fr]">
        <nav aria-label="Settings sections" className="flex gap-1 overflow-x-auto lg:flex-col">
          {tabs.map((t) => (
            <NavLink
              key={t.id}
              to={`/settings/${t.id}`}
              className={({ isActive }) =>
                cn(
                  "rounded-xl px-3 py-2 text-sm whitespace-nowrap hover:bg-muted",
                  isActive && "bg-muted font-medium",
                )
              }
            >
              {t.title}
            </NavLink>
          ))}
        </nav>
        <div className="flex min-w-0 flex-col gap-4">
          <Content id={section} />
        </div>
      </div>
    </Page>
  );
}
