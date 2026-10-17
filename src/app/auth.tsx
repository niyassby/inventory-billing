import { GalleryVerticalEndIcon } from "@hugeicons/core-free-icons/index";
import { HugeiconsIcon } from "@hugeicons/react";
import { APP_NAME } from "@inventory/shared";
import { LoginForm } from "@/components/auth/login-form";

export default function Auth() {
  return (
    <div className="grid min-h-svh lg:grid-cols-2">
      <div className="flex flex-col gap-4 p-6 md:p-10">
        <div className="flex items-center justify-center gap-2 font-medium md:justify-start">
          <div className="flex size-6 items-center justify-center rounded-md bg-primary text-primary-foreground">
            <HugeiconsIcon size={16} icon={GalleryVerticalEndIcon} strokeWidth={2} />
          </div>
          {APP_NAME}
        </div>
        <div className="flex flex-1 items-center justify-center">
          <div className="w-full max-w-xs">
            <LoginForm />
          </div>
        </div>
      </div>
      {/* Local decorative panel: the app must not load remote images (works offline, no tracking). */}
      <div
        aria-hidden="true"
        className="relative hidden overflow-hidden bg-linear-to-br from-primary/80 via-primary/50 to-muted lg:block"
      >
        <div className="absolute inset-0 bg-[radial-gradient(circle_at_30%_20%,rgba(255,255,255,0.2),transparent_55%)]" />
      </div>
    </div>
  );
}
