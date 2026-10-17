import React, { useState } from "react";
import { useNavigate } from "react-router-dom";
import { invoke } from "@tauri-apps/api/core";
import { useStore } from "@/hooks/use-store";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Label } from "@/components/ui/label";
import { HugeiconsIcon } from "@hugeicons/react";
import {
  UserIcon,
  Database02Icon,
  Configuration02Icon,
  BookOpen01Icon
} from "@hugeicons/core-free-icons";

export default function Maintenance() {
  const navigate = useNavigate();
  const token = useStore((state) => state.token);

  // Backup / Restore paths
  const [backupPath, setBackupPath] = useState("C:\\Users\\Public\\inventory_backup.db");
  const [restorePath, setRestorePath] = useState("C:\\Users\\Public\\inventory_backup.db");

  // Database Backup export
  const handleBackupExport = async () => {
    try {
      await invoke("backup_database_cmd", { token, destPath: backupPath });
      alert("Database backup completed successfully at:\n" + backupPath);
    } catch (e) {
      alert("Database backup failed: " + e);
    }
  };

  // Database Restore import
  const handleRestoreImport = async () => {
    if (!window.confirm("WARNING: Importing a database restore overrides the current active records. Proceed?")) return;
    try {
      await invoke("restore_database_cmd", { token, srcPath: restorePath });
      alert("Database restored successfully! The application will refresh.");
      window.location.reload();
    } catch (e) {
      alert("Database restore failed: " + e);
    }
  };

  return (
    <div className="space-y-6 pb-8 font-sans">
      <div className="flex flex-col md:flex-row justify-between items-start md:items-center gap-4 border-b border-border/40 pb-4">
        <div>
          <h1 className="text-3xl font-bold tracking-tight">System Settings</h1>
          <p className="text-muted-foreground">Configure profile details, preferences, accounts control, and database maintenance.</p>
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
            variant="default"
            onClick={() => navigate("/settings/maintenance")}
            className="flex items-center gap-1.5"
          >
            <HugeiconsIcon icon={Database02Icon} size={16} /> DB Maintenance
          </Button>
        </div>
      </div>

      <div className="grid gap-6 md:grid-cols-2">
        {/* Backup Database */}
        <Card className="border-border/40 bg-card/65 backdrop-blur-sm">
          <CardHeader>
            <CardTitle>Export Database Backup</CardTitle>
            <CardDescription>
              Back up your active inventory catalog, invoices ledger, and audit logs. Compiles using SQLite online backup.
            </CardDescription>
          </CardHeader>
          <CardContent className="space-y-4">
            <div className="space-y-2">
              <Label htmlFor="b_path">Local Target Path *</Label>
              <Input
                id="b_path"
                value={backupPath}
                onChange={(e) => setBackupPath(e.target.value)}
                required
              />
            </div>
            <Button onClick={handleBackupExport} className="w-full flex items-center justify-center gap-1.5">
              <HugeiconsIcon icon={Database02Icon} size={16} /> Export Backup File
            </Button>
          </CardContent>
        </Card>

        {/* Restore Database */}
        <Card className="border-border/40 bg-card/65 backdrop-blur-sm">
          <CardHeader>
            <CardTitle>Import Database Restore</CardTitle>
            <CardDescription>
              Restore the database state from a compiled `.db` file. This overrides active ledger values.
            </CardDescription>
          </CardHeader>
          <CardContent className="space-y-4">
            <div className="space-y-2">
              <Label htmlFor="r_path">Source Backup Path *</Label>
              <Input
                id="r_path"
                value={restorePath}
                onChange={(e) => setRestorePath(e.target.value)}
                required
              />
            </div>
            <Button onClick={handleRestoreImport} variant="destructive" className="w-full flex items-center justify-center gap-1.5">
              <HugeiconsIcon icon={Database02Icon} size={16} /> Import Database Restore
            </Button>
          </CardContent>
        </Card>
      </div>
    </div>
  );
}
