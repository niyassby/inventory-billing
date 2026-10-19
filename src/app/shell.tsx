import { useEffect } from "react";
import { Link, Outlet, useLocation, useNavigate } from "react-router-dom";
import { APP_NAME } from "@inventory/shared";
import { LogOut, Store } from "lucide-react";
import {
  Sidebar,
  SidebarContent,
  SidebarFooter,
  SidebarGroup,
  SidebarGroupLabel,
  SidebarHeader,
  SidebarInset,
  SidebarMenu,
  SidebarMenuButton,
  SidebarMenuItem,
  SidebarProvider,
  SidebarRail,
  SidebarTrigger,
} from "@/components/ui/sidebar";
import { Separator } from "@/components/ui/separator";
import { Button } from "@/components/ui/button";
import { Kbd } from "@/components/ui/kbd";
import { ConnectionBadge } from "@/components/connection-badge";
import { usePublicSettings } from "@/lib/business";
import { useSession } from "@/stores/session";
import { activeItem, visibleNav } from "./nav";

function AppSidebar() {
  const { pathname } = useLocation();
  const user = useSession((s) => s.user);
  const signOut = useSession((s) => s.signOut);
  const settings = usePublicSettings();
  const groups = visibleNav(user?.permissions ?? []);
  const active = activeItem(pathname);
  return (
    <Sidebar collapsible="icon">
      <SidebarHeader>
        <SidebarMenu>
          <SidebarMenuItem>
            <SidebarMenuButton size="lg" render={<Link to="/" />}>
              <div className="flex aspect-square size-8 items-center justify-center rounded-lg bg-primary text-primary-foreground">
                <Store className="size-4" aria-hidden />
              </div>
              <div className="grid flex-1 text-left text-sm leading-tight">
                <span className="truncate font-semibold">
                  {settings.data?.business.name || APP_NAME}
                </span>
                <span className="truncate text-xs text-muted-foreground">{APP_NAME}</span>
              </div>
            </SidebarMenuButton>
          </SidebarMenuItem>
        </SidebarMenu>
      </SidebarHeader>
      <SidebarContent>
        {groups.map((group) => (
          <SidebarGroup key={group.label}>
            <SidebarGroupLabel>{group.label}</SidebarGroupLabel>
            <SidebarMenu>
              {group.items.map((item) => (
                <SidebarMenuItem key={item.to}>
                  <SidebarMenuButton
                    tooltip={item.title}
                    isActive={active?.to === item.to}
                    render={<Link to={item.to} />}
                  >
                    <item.icon aria-hidden />
                    <span>{item.title}</span>
                    {item.shortcut && <Kbd className="ml-auto">{item.shortcut}</Kbd>}
                  </SidebarMenuButton>
                </SidebarMenuItem>
              ))}
            </SidebarMenu>
          </SidebarGroup>
        ))}
      </SidebarContent>
      <SidebarFooter>
        <SidebarMenu>
          <SidebarMenuItem>
            <SidebarMenuButton tooltip="Sign out" onClick={() => void signOut()}>
              <LogOut aria-hidden />
              <span className="grid flex-1 text-left leading-tight">
                <span className="truncate text-sm font-medium">{user?.name}</span>
                <span className="truncate text-xs text-muted-foreground">
                  {user?.roleName} · Sign out
                </span>
              </span>
            </SidebarMenuButton>
          </SidebarMenuItem>
        </SidebarMenu>
      </SidebarFooter>
      <SidebarRail />
    </Sidebar>
  );
}

/** F2 opens the till from anywhere (the POS has its own keys). */
function useGlobalKeys() {
  const navigate = useNavigate();
  const canSell = useSession((s) => s.user?.permissions.includes("sales.create") ?? false);
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "F2" && canSell) {
        e.preventDefault();
        void navigate("/pos");
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [navigate, canSell]);
}

export function Shell() {
  const { pathname } = useLocation();
  const item = activeItem(pathname);
  useGlobalKeys();
  return (
    <SidebarProvider>
      <AppSidebar />
      <SidebarInset className="min-h-screen font-sans">
        <header className="sticky top-0 z-40 flex h-14 shrink-0 items-center gap-2 border-b border-border/40 bg-background/85 px-4 backdrop-blur-md">
          <SidebarTrigger className="-ml-1" />
          <Separator orientation="vertical" className="mx-1 h-5" />
          <span className="text-sm font-medium">{item?.title ?? ""}</span>
          <div className="ml-auto flex items-center gap-2">
            <ConnectionBadge />
            <Button size="sm" render={<Link to="/pos" />} nativeButton={false}>
              New sale
            </Button>
          </div>
        </header>
        <main className="flex-1">
          <Outlet />
        </main>
      </SidebarInset>
    </SidebarProvider>
  );
}
