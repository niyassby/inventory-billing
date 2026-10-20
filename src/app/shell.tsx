import { Fragment, useEffect, useRef } from "react";
import { Link, Outlet, useLocation, useNavigate } from "react-router-dom";
import { APP_NAME } from "@inventory/shared";
import { Plus, Search, Store } from "lucide-react";
import {
  Sidebar,
  SidebarContent,
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
import {
  Breadcrumb,
  BreadcrumbItem,
  BreadcrumbLink,
  BreadcrumbList,
  BreadcrumbPage,
  BreadcrumbSeparator,
} from "@/components/ui/breadcrumb";
import { Separator } from "@/components/ui/separator";
import { Button } from "@/components/ui/button";
import { Kbd } from "@/components/ui/kbd";
import { ConnectionBadge } from "@/components/connection-badge";
import { usePublicSettings } from "@/lib/business";
import { useSession } from "@/stores/session";
import { ThemeButton } from "./appearance-menu";
import { CommandPalette } from "./command-palette";
import { activeItem, breadcrumbs, visibleNav } from "./nav";
import { UserMenu } from "./user-menu";

function AppSidebar() {
  const { pathname } = useLocation();
  const user = useSession((s) => s.user);
  const settings = usePublicSettings();
  const groups = visibleNav(user?.permissions ?? []);
  const active = activeItem(pathname);
  return (
    <Sidebar collapsible="icon">
      <SidebarHeader className="border-b border-sidebar-border">
        <SidebarMenu>
          <SidebarMenuItem>
            <SidebarMenuButton
              size="lg"
              render={<Link to="/" />}
              className="data-active:before:hidden"
            >
              <div className="flex aspect-square size-8 items-center justify-center rounded-md bg-primary text-primary-foreground shadow-xs">
                <Store className="size-4" aria-hidden />
              </div>
              <div className="grid flex-1 text-left leading-tight">
                <span className="truncate text-sm font-semibold">
                  {settings.data?.business.name || APP_NAME}
                </span>
                <span className="truncate text-caption">{APP_NAME}</span>
              </div>
            </SidebarMenuButton>
          </SidebarMenuItem>
        </SidebarMenu>
      </SidebarHeader>
      <SidebarContent className="pt-1">
        {groups.map((group) => (
          <SidebarGroup key={group.label}>
            {group.label !== "Home" && <SidebarGroupLabel>{group.label}</SidebarGroupLabel>}
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

function Crumbs({ pathname }: { pathname: string }) {
  const crumbs = breadcrumbs(pathname);
  if (crumbs.length === 0) return null;
  return (
    <Breadcrumb>
      <BreadcrumbList>
        {crumbs.map((c, i) => (
          <Fragment key={`${c.label}-${i}`}>
            {i > 0 && <BreadcrumbSeparator />}
            <BreadcrumbItem>
              {i === crumbs.length - 1 ? (
                <BreadcrumbPage>{c.label}</BreadcrumbPage>
              ) : c.to ? (
                <BreadcrumbLink render={<Link to={c.to} />}>{c.label}</BreadcrumbLink>
              ) : (
                <span className="text-muted-foreground">{c.label}</span>
              )}
            </BreadcrumbItem>
          </Fragment>
        ))}
      </BreadcrumbList>
    </Breadcrumb>
  );
}

export function Shell() {
  const { pathname } = useLocation();
  const canSell = useSession((s) => s.user?.permissions.includes("sales.create") ?? false);
  const item = activeItem(pathname);
  useGlobalKeys();
  const first = useRef(true);
  // Each screen names the window, and focus moves to it so keyboard and screen-reader users
  // start at the top of the new page instead of on the link they used.
  useEffect(() => {
    document.title = item ? `${item.title} · ${APP_NAME}` : APP_NAME;
    if (first.current) {
      first.current = false;
      return;
    }
    document.getElementById("main")?.focus({ preventScroll: true });
  }, [pathname, item]);
  return (
    <SidebarProvider>
      <a href="#main" className="skip-link">
        Skip to content
      </a>
      <CommandPalette />
      <AppSidebar />
      <SidebarInset className="min-h-screen font-sans">
        <header className="sticky top-0 z-40 flex h-12 shrink-0 items-center gap-2 border-b border-border bg-background/85 px-3 backdrop-blur-md">
          <SidebarTrigger className="-ml-1 text-muted-foreground" />
          <Separator orientation="vertical" className="my-auto h-6" />
          <Crumbs pathname={pathname} />
          <div className="ml-auto flex items-center gap-1.5">
            <Button
              variant="outline"
              size="sm"
              className="hidden w-56 justify-start gap-2 font-normal text-muted-foreground lg:inline-flex"
              onClick={() =>
                window.dispatchEvent(new KeyboardEvent("keydown", { key: "k", ctrlKey: true }))
              }
            >
              <Search aria-hidden />
              Search
              <Kbd className="ml-auto">Ctrl K</Kbd>
            </Button>
            <Button
              variant="ghost"
              size="icon-sm"
              className="lg:hidden"
              aria-label="Search"
              onClick={() =>
                window.dispatchEvent(new KeyboardEvent("keydown", { key: "k", ctrlKey: true }))
              }
            >
              <Search />
            </Button>
            <ConnectionBadge />
            <ThemeButton />
            {canSell && (
              <Button size="sm" render={<Link to="/pos" />} nativeButton={false}>
                <Plus aria-hidden />
                New sale
              </Button>
            )}
            <UserMenu />
          </div>
        </header>
        <main id="main" tabIndex={-1} className="flex-1 outline-none">
          <Outlet />
        </main>
      </SidebarInset>
    </SidebarProvider>
  );
}
