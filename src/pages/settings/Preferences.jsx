import React from "react";
import { useNavigate } from "react-router-dom";
import { useStore } from "@/hooks/use-store";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Label } from "@/components/ui/label";
import { HugeiconsIcon } from "@hugeicons/react";
import {
  UserIcon,
  Database02Icon,
  Configuration02Icon,
  BookOpen01Icon,
} from "@hugeicons/core-free-icons";

export default function Preferences() {
  const navigate = useNavigate();
  const currencySymbol = useStore((state) => state.currencySymbol);
  const setCurrencySymbol = useStore((state) => state.setCurrencySymbol);

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
            variant="default"
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
          <CardTitle>System Preferences</CardTitle>
          <CardDescription>Set layout tokens and local billing parameters.</CardDescription>
        </CardHeader>
        <CardContent className="space-y-4">
          <div className="space-y-2">
            <Label>Active Currency Symbol</Label>
            <select
              className="w-full rounded-md border border-border/40 bg-card p-2 text-sm text-foreground"
              value={currencySymbol}
              onChange={(e) => setCurrencySymbol(e.target.value)}
            >
              <option value="$">US Dollar ($)</option>
              <option value="₹">Indian Rupee (₹)</option>
              <option value="€">Euro (€)</option>
              <option value="£">British Pound (£)</option>
              <option value="¥">Japanese Yen (¥)</option>
            </select>
          </div>
          <div className="rounded border bg-muted/10 p-3 text-xs text-muted-foreground">
            These choices sync instantly to POS registers, product grids, billing calculations, and
            financial reports.
          </div>
        </CardContent>
      </Card>
    </div>
  );
}
