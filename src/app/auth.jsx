import { GalleryVerticalEndIcon } from "@hugeicons/core-free-icons/index"

import { LoginForm } from "@/components/auth/login-form"
import { HugeiconsIcon } from "@hugeicons/react"

export default function Auth() {
  return (
    <div className="grid min-h-svh lg:grid-cols-2">
      <div className="flex flex-col gap-4 p-6 md:p-10">
        <div className="flex justify-center gap-2 md:justify-start">
          <a href="/" className="flex items-center gap-2 font-medium">
            <div className="flex size-6 items-center justify-center rounded-md bg-primary text-primary-foreground">
            <HugeiconsIcon size={16} icon={GalleryVerticalEndIcon} strokeWidth={2} />
            </div>
            Acme Inc. 
          </a>
        </div>
        <div className="flex flex-1 items-center justify-center">
          <div className="w-full max-w-xs">
            <LoginForm />
          </div>
        </div>
      </div>
      <div className="relative hidden bg-muted lg:block">
        <img
          src="https://miro.medium.com/v2/resize:fit:1400/1*j0gjLpN1aaHuRowTKgr6yQ.jpeg"
          alt="Image"
          className="absolute  inset-0 h-full w-full object-cover dark:brightness-[0.2] dark:grayscale"
        />
      </div>
    </div>
  )
}
