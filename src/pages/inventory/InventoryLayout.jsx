import { Button } from '@/components/ui/button'
import { HugeiconsIcon } from '@hugeicons/react'
import {
  ArchiveIcon,
  Tag01Icon,
  ShuffleIcon,
  BarcodeIcon
} from "@hugeicons/core-free-icons";
import React from 'react'
import { Outlet, useLocation, useNavigate } from 'react-router-dom'

const links = [
    {
        path: "/inventory/products",
        name: "Products",
        icon: ArchiveIcon
    },
    {
        path: "/inventory/categories",
        name: "Categories & Brands",
        icon: Tag01Icon
    },
    {
        path: "/inventory/adjustments",
        name: "Adjustments",
        icon: ShuffleIcon
    },
    {
        path: "/inventory/barcodes",
        name: "Barcodes",
        icon: BarcodeIcon
    }
]

function InventoryLayout() {
    const navigate = useNavigate()
    const location = useLocation()
  return (
    <div className='space-y-6 pb-8 font-sans animate-in fade-in duration-300 slide-in-from-bottom-2'>
        <div className="flex flex-col md:flex-row justify-between items-start md:items-center gap-4 border-b border-border/40 pb-4">
        <div>
          <h1 className="text-3xl font-bold tracking-tight">Inventory</h1>
          <p className="text-muted-foreground">Manage products catalogue, categorize, adjust stocks, and print barcode sheets.</p>
        </div>
        <div className="flex flex-wrap gap-2 print:hidden">
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

export default InventoryLayout