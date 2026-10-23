import { useState } from "react";
import type { Api } from "@inventory/shared";
import { CalendarClock, Save, Trash2 } from "lucide-react";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Field, FieldDescription, FieldLabel } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { SelectField } from "@/components/ui/select-field";
import { confirm } from "@/components/app/dialogs";
import { showDateTime } from "@/lib/format";
import { fieldError, useAction, useCommand } from "@/lib/queries";
import { run } from "@/lib/run";
import { useCan } from "@/stores/session";

/**
 * Saved views of one report for the signed-in user: apply one, save the current settings as a
 * view, delete one, and (on the hub) schedule its CSV export to a folder.
 */
export function SavedViews({
  report,
  current,
  onApply,
}: {
  report: string;
  current: Api.ViewParams;
  onApply: (params: Api.ViewParams) => void;
}) {
  const views = useCommand("report_views_list", {});
  const [chosen, setChosen] = useState("");
  const [saving, setSaving] = useState(false);
  const [scheduling, setScheduling] = useState<Api.ReportView | null>(null);
  const canExport = useCan("reports.export");
  const canManage = useCan("settings.manage");
  const canSchedule = canExport && canManage;
  const remove = useAction("report_views_delete", {
    invalidates: ["report_views_list"],
    success: "View deleted",
    onSuccess: () => setChosen(""),
  });
  const mine = (views.data ?? []).filter((v) => v.report === report);
  const view = mine.find((v) => v.id === chosen);
  return (
    <div className="flex flex-wrap items-end gap-2">
      {mine.length > 0 && (
        <SelectField
          aria-label="Saved view"
          className="w-52"
          value={chosen}
          onValueChange={(id) => {
            setChosen(id);
            const v = mine.find((x) => x.id === id);
            if (v) onApply(v.params);
          }}
          options={[
            { value: "", label: "Saved views" },
            ...mine.map((v) => ({
              value: v.id,
              label: v.schedule ? `${v.name} (scheduled)` : v.name,
            })),
          ]}
        />
      )}
      <Button variant="outline" onClick={() => setSaving(true)}>
        <Save /> Save view
      </Button>
      {view && canSchedule && (
        <Button variant="outline" onClick={() => setScheduling(view)}>
          <CalendarClock /> Schedule export
        </Button>
      )}
      {view && (
        <Button
          size="icon"
          variant="ghost"
          aria-label={`Delete view ${view.name}`}
          onClick={run(async () => {
            const ok = await confirm({
              title: `Delete the view "${view.name}"?`,
              description: view.schedule ? "Its scheduled export stops too." : undefined,
              confirmLabel: "Delete",
              destructive: true,
            });
            if (ok !== false) remove.mutate({ input: { id: view.id } });
          })}
        >
          <Trash2 />
        </Button>
      )}
      {saving && (
        <SaveDialog
          report={report}
          params={current}
          onClose={() => setSaving(false)}
          onSaved={(v) => setChosen(v.id)}
        />
      )}
      {scheduling && <ScheduleDialog view={scheduling} onClose={() => setScheduling(null)} />}
    </div>
  );
}

function SaveDialog({
  report,
  params,
  onClose,
  onSaved,
}: {
  report: string;
  params: Api.ViewParams;
  onClose: () => void;
  onSaved: (v: Api.ReportView) => void;
}) {
  const [name, setName] = useState("");
  const save = useAction("report_views_save", {
    invalidates: ["report_views_list"],
    success: (v) => `View "${v.name}" saved`,
    onSuccess: (v) => {
      onSaved(v);
      onClose();
    },
  });
  return (
    <Dialog open onOpenChange={(o) => !o && onClose()}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Save this view</DialogTitle>
          <DialogDescription>
            The period, grouping and filters are kept under a name of your own. Other users don't
            see your views.
          </DialogDescription>
        </DialogHeader>
        <form
          id="view-form"
          onSubmit={(e) => {
            e.preventDefault();
            save.mutate({ input: { id: null, expectedUpdatedAt: null, report, name, params } });
          }}
        >
          <Field>
            <FieldLabel htmlFor="view-name">Name</FieldLabel>
            <Input
              id="view-name"
              autoFocus
              maxLength={80}
              value={name}
              onChange={(e) => setName(e.target.value)}
            />
            <span className="text-xs text-danger">{fieldError(save.error, "name")}</span>
          </Field>
        </form>
        <DialogFooter>
          <Button variant="ghost" onClick={onClose}>
            Cancel
          </Button>
          <Button type="submit" form="view-form" disabled={!name.trim() || save.isPending}>
            Save view
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

function ScheduleDialog({ view, onClose }: { view: Api.ReportView; onClose: () => void }) {
  const [frequency, setFrequency] = useState(view.schedule?.frequency ?? "daily");
  const [folder, setFolder] = useState(view.schedule?.folder ?? "");
  const schedule = useAction("report_views_schedule", {
    invalidates: ["report_views_list"],
    success: "Export scheduled",
    onSuccess: onClose,
  });
  const stop = useAction("report_views_schedule", {
    invalidates: ["report_views_list"],
    success: "Export stopped",
    onSuccess: onClose,
  });
  const runNow = useAction("report_views_run_now", { success: (file) => `Written: ${file}` });
  return (
    <Dialog open onOpenChange={(o) => !o && onClose()}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Schedule "{view.name}"</DialogTitle>
          <DialogDescription>
            The hub writes this view as a CSV file into a folder: daily (yesterday), weekly (last
            Monday to Sunday) or monthly (last month). The app must be running on the hub.
          </DialogDescription>
        </DialogHeader>
        <div className="flex flex-col gap-4">
          <Field>
            <FieldLabel htmlFor="sch-frequency">How often</FieldLabel>
            <SelectField
              id="sch-frequency"
              value={frequency}
              onValueChange={setFrequency}
              options={[
                { value: "daily", label: "Every day (yesterday's figures)" },
                { value: "weekly", label: "Every week (last week)" },
                { value: "monthly", label: "Every month (last month)" },
              ]}
            />
          </Field>
          <Field>
            <FieldLabel htmlFor="sch-folder">Folder on the hub PC</FieldLabel>
            <Input
              id="sch-folder"
              placeholder="D:\Reports"
              value={folder}
              onChange={(e) => setFolder(e.target.value)}
            />
            <FieldDescription>
              A full path that exists, for example a shared or synced folder.
            </FieldDescription>
            <span className="text-xs text-danger">{fieldError(schedule.error, "folder")}</span>
          </Field>
          {view.schedule?.lastRunAt && (
            <p className="text-xs text-muted-foreground">
              Last run {showDateTime(view.schedule.lastRunAt)} ({view.schedule.lastStatus ?? ""}
              {view.schedule.lastError ? `: ${view.schedule.lastError}` : ""})
            </p>
          )}
        </div>
        <DialogFooter className="flex-wrap gap-2">
          {view.schedule && (
            <>
              <Button
                variant="ghost"
                className="mr-auto"
                onClick={() =>
                  stop.mutate({
                    input: { viewId: view.id, frequency: null, folder: null, isActive: false },
                  })
                }
              >
                Stop exporting
              </Button>
              <Button variant="outline" onClick={() => runNow.mutate({ input: { id: view.id } })}>
                Export now
              </Button>
            </>
          )}
          <Button
            disabled={!folder.trim() || schedule.isPending}
            onClick={() =>
              schedule.mutate({
                input: { viewId: view.id, frequency, folder: folder.trim(), isActive: true },
              })
            }
          >
            Save schedule
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
