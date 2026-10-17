import { useEffect, useState } from "react";
import { useNavigate, useLocation, Link, Outlet } from "react-router-dom";
import { AppSidebar } from "@/components/sidebar/app-sidebar";
import {
  Breadcrumb,
  BreadcrumbItem,
  BreadcrumbLink,
  BreadcrumbList,
  BreadcrumbPage,
  BreadcrumbSeparator,
} from "@/components/ui/breadcrumb";
import { Separator } from "@/components/ui/separator";
import { SidebarInset, SidebarProvider, SidebarTrigger } from "@/components/ui/sidebar";
import { ScrollArea } from "@/components/ui/scroll-area";
import { useStore } from "@/hooks/use-store";
import { Button } from "@/components/ui/button";
import { HugeiconsIcon } from "@hugeicons/react";
import { Notification } from "@hugeicons/core-free-icons/index";

export default function Layout() {
  const navigate = useNavigate();
  const location = useLocation();
  const { isAuthenticated, authLoading, checkAuth, user } = useStore();

  useEffect(() => {
    checkAuth();
  }, [checkAuth]);

  useEffect(() => {
    if (!authLoading && !isAuthenticated) {
      navigate("/auth");
    }
  }, [authLoading, isAuthenticated, navigate]);

  if (authLoading) {
    return (
      <div className="flex h-screen w-screen items-center justify-center bg-background font-sans text-foreground">
        <div className="flex flex-col items-center gap-4">
          <div className="h-12 w-12 animate-spin rounded-full border-4 border-primary border-t-transparent"></div>
          <p className="animate-pulse text-sm font-semibold text-muted-foreground">
            Connecting to Secure Database...
          </p>
        </div>
      </div>
    );
  }

  if (!isAuthenticated) {
    return null; // Prevents render flashes before redirecting
  }

  // Derive breadcrumbs from path
  const pathParts = location.pathname.split("/").filter(Boolean);
  const getBreadcrumbTitle = (part) => {
    return part.replace(/-/g, " ").replace(/\b\w/g, (c) => c.toUpperCase());
  };

  return (
    <SidebarProvider>
      <AppSidebar />
      <ScrollArea className="h-screen w-full">
        <SidebarInset>
          <header className="sticky top-0 right-0 z-50 flex h-16 w-full shrink-0 items-center gap-2 border-b border-border/40 bg-background/85 font-sans backdrop-blur-md transition-[width,height] ease-linear group-has-data-[collapsible=icon]/sidebar-wrapper:h-12">
            <div className="flex w-full items-center justify-between gap-2 px-4">
              <div className="flex items-center gap-2">
                <SidebarTrigger className="-ml-1" />
                <Separator orientation="vertical" className="mr-2" />
                <Breadcrumb>
                  <BreadcrumbList>
                    <BreadcrumbItem className="hidden md:block">
                      <BreadcrumbLink render={<Link to="/dashboard">App</Link>} />
                    </BreadcrumbItem>
                    {pathParts.map((part, index) => {
                      const url = `/${pathParts.slice(0, index + 1).join("/")}`;
                      const isLast = index === pathParts.length - 1;
                      return (
                        <span key={url} className="flex items-center gap-1.5 md:gap-2">
                          <BreadcrumbSeparator className="hidden md:block" />
                          <BreadcrumbItem>
                            {isLast ? (
                              <BreadcrumbPage className="font-semibold text-foreground">
                                {getBreadcrumbTitle(part)}
                              </BreadcrumbPage>
                            ) : (
                              <BreadcrumbLink
                                render={<Link to={url}>{getBreadcrumbTitle(part)}</Link>}
                                className="hidden md:block"
                              />
                            )}
                          </BreadcrumbItem>
                        </span>
                      );
                    })}
                  </BreadcrumbList>
                </Breadcrumb>
              </div>
              <div>
                <Button variant="outline" size="icon">
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
  );
}
