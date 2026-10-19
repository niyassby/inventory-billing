import { Link, Outlet, useLocation } from "react-router-dom";
import { ConnectionBadge } from "@/components/connection-badge";
import { HugeiconsIcon } from "@hugeicons/react";
import { Notification } from "@hugeicons/core-free-icons/index";
import { AppSidebar } from "@/components/sidebar/app-sidebar";
import {
  Breadcrumb,
  BreadcrumbItem,
  BreadcrumbLink,
  BreadcrumbList,
  BreadcrumbPage,
  BreadcrumbSeparator,
} from "@/components/ui/breadcrumb";
import { Button } from "@/components/ui/button";
import { ScrollArea } from "@/components/ui/scroll-area";
import { Separator } from "@/components/ui/separator";
import { SidebarInset, SidebarProvider, SidebarTrigger } from "@/components/ui/sidebar";
import { RequireAuth } from "./require-auth";

function titleFromSegment(segment: string): string {
  return segment.replace(/-/g, " ").replace(/\b\w/g, (c) => c.toUpperCase());
}

function Breadcrumbs() {
  const location = useLocation();
  const parts = location.pathname.split("/").filter(Boolean);

  return (
    <Breadcrumb>
      <BreadcrumbList>
        <BreadcrumbItem className="hidden md:block">
          <BreadcrumbLink render={<Link to="/dashboard">App</Link>} />
        </BreadcrumbItem>
        {parts.map((part, index) => {
          const url = `/${parts.slice(0, index + 1).join("/")}`;
          const isLast = index === parts.length - 1;
          return (
            <span key={url} className="flex items-center gap-1.5 md:gap-2">
              <BreadcrumbSeparator className="hidden md:block" />
              <BreadcrumbItem>
                {isLast ? (
                  <BreadcrumbPage className="font-semibold text-foreground">
                    {titleFromSegment(part)}
                  </BreadcrumbPage>
                ) : (
                  <BreadcrumbLink
                    render={<Link to={url}>{titleFromSegment(part)}</Link>}
                    className="hidden md:block"
                  />
                )}
              </BreadcrumbItem>
            </span>
          );
        })}
      </BreadcrumbList>
    </Breadcrumb>
  );
}

export default function Layout() {
  return (
    <RequireAuth>
      <SidebarProvider>
        <AppSidebar />
        <ScrollArea className="h-screen w-full">
          <SidebarInset>
            <header className="sticky top-0 right-0 z-50 flex h-16 w-full shrink-0 items-center gap-2 border-b border-border/40 bg-background/85 font-sans backdrop-blur-md transition-[width,height] ease-linear group-has-data-[collapsible=icon]/sidebar-wrapper:h-12">
              <div className="flex w-full items-center justify-between gap-2 px-4">
                <div className="flex items-center gap-2">
                  <SidebarTrigger className="-ml-1" />
                  <Separator orientation="vertical" className="mr-2" />
                  <Breadcrumbs />
                </div>
                <div className="flex items-center gap-2">
                  <ConnectionBadge />
                  <Button variant="outline" size="icon" aria-label="Notifications">
                    <HugeiconsIcon icon={Notification} size={16} strokeWidth={2} />
                  </Button>
                </div>
              </div>
            </header>
            <div className="h-full">
              <Outlet />
            </div>
          </SidebarInset>
        </ScrollArea>
      </SidebarProvider>
    </RequireAuth>
  );
}
