import { Button } from '@/components/ui/button'
import { FileIcon, LegalDocument01Icon, RefreshCwOffIcon } from '@hugeicons/core-free-icons/index';
import { HugeiconsIcon } from '@hugeicons/react';
import React from 'react'
import { Outlet, useLocation, useNavigate } from 'react-router-dom'

function SalesLayout() {
    const navigate = useNavigate();
    const location = useLocation();

    const links = [
        {
            path: "/sales/invoices",
            name: "Invoices List",
            icon: LegalDocument01Icon
        },
        {
            path: "/sales/quotations",
            name: "Quotations",
            icon: FileIcon
        },
        {
            path: "/sales/returns",
            name: "Sales Returns & Credit Notes",
            icon: RefreshCwOffIcon
        }
    ]
  return (
    <div className='space-y-6 pb-8 font-sans animate-in fade-in duration-300 slide-in-from-bottom-2'>
        <div className="flex flex-col md:flex-row justify-between items-start md:items-center gap-4 border-b border-border/40 pb-4">
        <div>
          <h1 className="text-3xl font-bold tracking-tight">Billing & Sales</h1>
          <p className="text-muted-foreground">Manage invoices, prepare customer quotations, and track store revenues.</p>
        </div>
        <div className="flex gap-2 print:hidden">
          {links.map((link) => (
            <Button
            key={link.path}
            variant={link.path === location.pathname ? "default" : "outline"}
            onClick={() => navigate(link.path)}
            className="flex items-center gap-1.5"
          >
            <HugeiconsIcon icon={link.icon} size={16} strokeWidth={2} /> {link.name}
          </Button>
          ))}
        </div>
      </div>
      <Outlet />
    </div>
  )
}

export default SalesLayout