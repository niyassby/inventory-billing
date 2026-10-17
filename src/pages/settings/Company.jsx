import React, { useState } from "react";
import { useNavigate } from "react-router-dom";
import { useStore } from "@/hooks/use-store";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { HugeiconsIcon } from "@hugeicons/react";
import {
  UserIcon,
  Database02Icon,
  Configuration02Icon,
  BookOpen01Icon,
} from "@hugeicons/core-free-icons";
import { Label } from "@/components/ui/label";

export default function Company() {
  const navigate = useNavigate();
  const user = useStore((state) => state.user);
  const isAdmin = user?.role === "admin";

  // Company Profile Settings
  const [companyDetails, setCompanyDetails] = useState({
    name: localStorage.getItem("company_name") || "Acme Wholesale Ltd",
    phone: localStorage.getItem("company_phone") || "+1 (555) 019-2834",
    address: localStorage.getItem("company_address") || "456 Commerce Avenue, Suite 100",
    taxNo: localStorage.getItem("company_tax_no") || "TAX-998877-XX",
  });

  // Company details save
  const handleCompanySave = (e) => {
    e.preventDefault();
    localStorage.setItem("company_name", companyDetails.name);
    localStorage.setItem("company_phone", companyDetails.phone);
    localStorage.setItem("company_address", companyDetails.address);
    localStorage.setItem("company_tax_no", companyDetails.taxNo);
    alert("Company settings saved locally!");
  };

  return (
    <div className="space-y-6 pb-8 font-sans">
      <div className="flex flex-col items-start justify-between gap-4 border-b border-border/40 pb-4 md:flex-row md:items-center">
        <div>
          <h1 className="text-3xl font-bold tracking-tight">System Settings</h1>
          <p className="text-muted-foreground">
            Configure profile details, preferences, accounts control, and database maintenance.
          </p>
        </div>
        <div className="flex flex-wrap gap-2 print:hidden">
          <Button
            variant="default"
            onClick={() => navigate("/settings/company")}
            className="flex items-center gap-1.5"
          >
            <HugeiconsIcon icon={BookOpen01Icon} size={16} /> Company Details
          </Button>
          <Button
            variant="outline"
            onClick={() => navigate("/settings/users")}
            className="flex items-center gap-1.5"
          >
            <HugeiconsIcon icon={UserIcon} size={16} /> Users Control
          </Button>
          <Button
            variant="outline"
            onClick={() => navigate("/settings/preferences")}
            className="flex items-center gap-1.5"
          >
            <HugeiconsIcon icon={Configuration02Icon} size={16} /> Preferences
          </Button>
          <Button
            variant="outline"
            onClick={() => navigate("/settings/maintenance")}
            className="flex items-center gap-1.5"
          >
            <HugeiconsIcon icon={Database02Icon} size={16} /> DB Maintenance
          </Button>
        </div>
      </div>

      <Card className="max-w-lg border-border/40 bg-card/65 backdrop-blur-sm">
        <CardHeader>
          <CardTitle>Merchant Profile Details</CardTitle>
          <CardDescription>
            Details printed on sales checkout receipts and invoice copies.
          </CardDescription>
        </CardHeader>
        <CardContent>
          <form onSubmit={handleCompanySave} className="space-y-4">
            <div className="space-y-2">
              <Label htmlFor="comp_name">Company Name *</Label>
              <Input
                id="comp_name"
                value={companyDetails.name}
                onChange={(e) => setCompanyDetails({ ...companyDetails, name: e.target.value })}
                required
              />
            </div>
            <div className="space-y-2">
              <Label htmlFor="comp_phone">Contact Phone *</Label>
              <Input
                id="comp_phone"
                value={companyDetails.phone}
                onChange={(e) => setCompanyDetails({ ...companyDetails, phone: e.target.value })}
                required
              />
            </div>
            <div className="space-y-2">
              <Label htmlFor="comp_addr">Street Address *</Label>
              <Input
                id="comp_addr"
                value={companyDetails.address}
                onChange={(e) => setCompanyDetails({ ...companyDetails, address: e.target.value })}
                required
              />
            </div>
            <div className="space-y-2">
              <Label htmlFor="comp_tax">Tax Registration ID (GST/VAT)</Label>
              <Input
                id="comp_tax"
                value={companyDetails.taxNo}
                onChange={(e) => setCompanyDetails({ ...companyDetails, taxNo: e.target.value })}
              />
            </div>
            <Button type="submit" className="w-full">
              Save Profile
            </Button>
          </form>
        </CardContent>
      </Card>
    </div>
  );
}
