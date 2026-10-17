import React, { useState } from "react";
import { useNavigate } from "react-router-dom";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Label } from "@/components/ui/label";
import { HugeiconsIcon } from "@hugeicons/react";
import {
  UserIcon,
  Database02Icon,
  Configuration02Icon,
  BookOpen01Icon,
} from "@hugeicons/core-free-icons";

export default function Templates() {
  const navigate = useNavigate();

  const [template, setTemplate] = useState({
    header: "Welcome to our Store!",
    footer: "Thank you for your business. Please come again!",
  });

  const handleSave = (e) => {
    e.preventDefault();
    localStorage.setItem("receipt_header", template.header);
    localStorage.setItem("receipt_footer", template.footer);
    alert("Templates updated successfully!");
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
            variant="outline"
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
          <CardTitle>Receipt & Invoice Templates</CardTitle>
          <CardDescription>
            Custom messages to print at the top/bottom of transaction sheets.
          </CardDescription>
        </CardHeader>
        <CardContent>
          <form onSubmit={handleSave} className="space-y-4">
            <div className="space-y-2">
              <Label htmlFor="t_hdr">Receipt Header Text</Label>
              <Input
                id="t_hdr"
                value={template.header}
                onChange={(e) => setTemplate({ ...template, header: e.target.value })}
              />
            </div>
            <div className="space-y-2">
              <Label htmlFor="t_ftr">Receipt Footer Text</Label>
              <Input
                id="t_ftr"
                value={template.footer}
                onChange={(e) => setTemplate({ ...template, footer: e.target.value })}
              />
            </div>
            <Button type="submit" className="w-full">
              Save Templates
            </Button>
          </form>
        </CardContent>
      </Card>
    </div>
  );
}
