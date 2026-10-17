import React, { useState, useEffect } from "react";
import { useNavigate } from "react-router-dom";
import { invoke } from "@tauri-apps/api/core";
import { useStore } from "@/stores/app-store";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Card, CardContent } from "@/components/ui/card";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogFooter,
  DialogDescription,
} from "@/components/ui/dialog";
import { Label } from "@/components/ui/label";
import { ScrollArea } from "@/components/ui/scroll-area";
import { HugeiconsIcon } from "@hugeicons/react";
import {
  Add01Icon,
  BookOpen02Icon,
  ShuffleIcon,
  DollarSignIcon,
  BadgePercentIcon,
} from "@hugeicons/core-free-icons";

export default function Journals() {
  const navigate = useNavigate();
  const token = useStore((state) => state.token);
  const currencySymbol = useStore((state) => state.currencySymbol);
  const userRole = useStore((state) => state.user?.role);

  const isReadOnly = userRole === "staff";

  // Data States
  const [accounts, setAccounts] = useState([]);
  const [journals, setJournals] = useState([]);
  const [loading, setLoading] = useState(true);

  // Modals States
  const [isJournalModalOpen, setIsJournalModalOpen] = useState(false);
  const [journalDesc, setJournalDesc] = useState("");
  const [journalRef, setJournalRef] = useState("");
  const [journalLines, setJournalLines] = useState([
    { account_id: "", debit: 0, credit: 0 },
    { account_id: "", debit: 0, credit: 0 },
  ]);

  // Fetch Data
  const fetchData = async () => {
    setLoading(true);
    try {
      const list = await invoke("list_journal_entries_cmd", { token });
      setJournals(list);
      const accList = await invoke("list_chart_of_accounts_cmd", { token });
      setAccounts(accList);
    } catch (e) {
      console.error("Failed to load journal entry data:", e);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    fetchData();
  }, [token]);

  // Journal Entry lines updates
  const updateJournalLine = (index, field, value) => {
    const updated = [...journalLines];
    updated[index][field] = value;
    setJournalLines(updated);
  };

  const addJournalLine = () => {
    setJournalLines([...journalLines, { account_id: "", debit: 0, credit: 0 }]);
  };

  const removeJournalLine = (index) => {
    setJournalLines(journalLines.filter((_, i) => i !== index));
  };

  // Debits Credits Balancing verification
  const totalDebits = journalLines.reduce((acc, l) => acc + (parseFloat(l.debit) || 0), 0);
  const totalCredits = journalLines.reduce((acc, l) => acc + (parseFloat(l.credit) || 0), 0);
  const isBalanced = Math.abs(totalDebits - totalCredits) < 0.01 && totalDebits > 0;

  // Journal Entry Submit
  const handleJournalSubmit = async (e) => {
    e.preventDefault();
    if (!isBalanced)
      return alert(
        `Journal entry is not balanced! Debits: ${totalDebits}, Credits: ${totalCredits}`,
      );

    try {
      await invoke("create_journal_entry_cmd", {
        token,
        entryDate: new Date().toISOString().split("T")[0],
        description: journalDesc,
        reference: journalRef || null,
        lines: journalLines.map((l) => ({
          account_id: l.account_id,
          debit: parseFloat(l.debit) || 0,
          credit: parseFloat(l.credit) || 0,
        })),
      });
      setIsJournalModalOpen(false);
      setJournalDesc("");
      setJournalRef("");
      setJournalLines([
        { account_id: "", debit: 0, credit: 0 },
        { account_id: "", debit: 0, credit: 0 },
      ]);
      fetchData();
    } catch (e) {
      alert("Error submitting journal entry: " + e);
    }
  };

  return (
    <div className="space-y-6 pb-8 font-sans">
      <div className="flex flex-col items-start justify-between gap-4 border-b border-border/40 pb-4 md:flex-row md:items-center">
        <div>
          <h1 className="text-3xl font-bold tracking-tight">Ledger & Accounting</h1>
          <p className="text-muted-foreground">
            Manage chart of accounts, draft balancing journal entries, log expenses, and configure
            tax rules.
          </p>
        </div>
        <div className="flex flex-wrap gap-2 print:hidden">
          <Button
            variant="outline"
            onClick={() => navigate("/accounting/chart-of-accounts")}
            className="flex items-center gap-1.5"
          >
            <HugeiconsIcon icon={BookOpen02Icon} size={16} /> Chart of Accounts
          </Button>
          <Button
            variant="default"
            onClick={() => navigate("/accounting/journals")}
            className="flex items-center gap-1.5"
          >
            <HugeiconsIcon icon={ShuffleIcon} size={16} /> Journal Entries
          </Button>
          <Button
            variant="outline"
            onClick={() => navigate("/accounting/expenses")}
            className="flex items-center gap-1.5"
          >
            <HugeiconsIcon icon={DollarSignIcon} size={16} /> Expense Logger
          </Button>
          <Button
            variant="outline"
            onClick={() => navigate("/accounting/taxes")}
            className="flex items-center gap-1.5"
          >
            <HugeiconsIcon icon={BadgePercentIcon} size={16} /> Tax Rules
          </Button>
        </div>
      </div>

      {loading ? (
        <div className="flex h-[40vh] items-center justify-center">
          <div className="h-8 w-8 animate-spin rounded-full border-4 border-primary border-t-transparent"></div>
        </div>
      ) : (
        <div className="space-y-4">
          <div className="flex items-center justify-between">
            <h3 className="text-lg font-semibold">Double Entry Journal Book</h3>
            {!isReadOnly && (
              <Button
                onClick={() => setIsJournalModalOpen(true)}
                className="flex items-center gap-1.5"
              >
                <HugeiconsIcon icon={Add01Icon} size={16} /> New Journal Entry
              </Button>
            )}
          </div>

          <Card className="border-border/40 bg-card/65 backdrop-blur-sm">
            <CardContent className="p-0">
              <div className="overflow-x-auto">
                <table className="w-full border-collapse text-left text-sm">
                  <thead>
                    <tr className="border-b border-border/40 bg-muted/20 text-xs text-muted-foreground uppercase">
                      <th className="p-4 font-medium">Date</th>
                      <th className="p-4 font-medium">Description</th>
                      <th className="p-4 font-medium">Reference</th>
                      <th className="p-4 text-right font-medium">Journal Lines</th>
                    </tr>
                  </thead>
                  <tbody className="divide-y divide-border/20">
                    {journals?.map((j) => {
                      const lines = j?.lines
                        ? typeof j.lines === "string"
                          ? JSON.parse(j.lines)
                          : j.lines
                        : [];
                      return (
                        <tr key={j.id} className="hover:bg-muted/10">
                          <td className="p-4 font-mono text-xs text-muted-foreground">
                            {j.entry_date}
                          </td>
                          <td className="p-4 font-medium">{j.description}</td>
                          <td className="p-4 font-mono text-xs text-muted-foreground">
                            {j.reference || "-"}
                          </td>
                          <td className="p-4 text-right text-xs">
                            <div className="space-y-1">
                              {lines?.map((l, idx) => (
                                <div key={idx} className="flex justify-end gap-3 font-mono">
                                  <span className="text-[10px] font-semibold text-muted-foreground uppercase">
                                    {l.account_name}:
                                  </span>
                                  {l.debit > 0 && (
                                    <span className="font-bold text-emerald-600">
                                      DR {currencySymbol}
                                      {l.debit.toFixed(2)}
                                    </span>
                                  )}
                                  {l.credit > 0 && (
                                    <span className="font-bold text-rose-600">
                                      CR {currencySymbol}
                                      {l.credit.toFixed(2)}
                                    </span>
                                  )}
                                </div>
                              ))}
                            </div>
                          </td>
                        </tr>
                      );
                    })}
                    {journals.length === 0 && (
                      <tr>
                        <td colSpan={4} className="py-8 text-center text-xs text-muted-foreground">
                          No journal entries logged.
                        </td>
                      </tr>
                    )}
                  </tbody>
                </table>
              </div>
            </CardContent>
          </Card>
        </div>
      )}

      {/* New Journal Entry Modal */}
      <Dialog open={isJournalModalOpen} onOpenChange={setIsJournalModalOpen}>
        <DialogContent className="font-sans sm:max-w-xl">
          <DialogHeader>
            <DialogTitle>New Double Entry Journal</DialogTitle>
            <DialogDescription>
              Debits must balance Credits before entry is validated.
            </DialogDescription>
          </DialogHeader>
          <form onSubmit={handleJournalSubmit} className="space-y-4">
            <div className="grid grid-cols-2 gap-4">
              <div className="space-y-2">
                <Label htmlFor="j_desc">Description *</Label>
                <Input
                  id="j_desc"
                  value={journalDesc}
                  onChange={(e) => setJournalDesc(e.target.value)}
                  required
                />
              </div>
              <div className="space-y-2">
                <Label htmlFor="j_ref">Reference #</Label>
                <Input
                  id="j_ref"
                  value={journalRef}
                  onChange={(e) => setJournalRef(e.target.value)}
                />
              </div>
            </div>

            {/* Dynamic ledger lines */}
            <div className="space-y-2">
              <div className="flex items-center justify-between">
                <Label className="text-xs font-semibold">Ledger Entry Lines</Label>
                <Button type="button" size="sm" variant="outline" onClick={addJournalLine}>
                  + Add Line
                </Button>
              </div>

              <ScrollArea className="h-48 rounded-lg border bg-card p-2">
                {journalLines.map((line, index) => (
                  <div
                    key={index}
                    className="mb-3 grid grid-cols-12 items-center gap-2 border-b border-border/20 pb-3 last:border-0 last:pb-0"
                  >
                    <div className="col-span-6">
                      <select
                        className="w-full rounded-md border border-border/40 bg-card p-2 text-xs text-foreground"
                        value={line.account_id}
                        onChange={(e) => updateJournalLine(index, "account_id", e.target.value)}
                        required
                      >
                        <option value="" disabled>
                          Choose Account
                        </option>
                        {accounts.map((a) => (
                          <option key={a.id} value={a.id}>
                            {a.code} - {a.name}
                          </option>
                        ))}
                      </select>
                    </div>
                    <div className="col-span-2.5">
                      <Input
                        placeholder="Debit"
                        type="number"
                        step="0.01"
                        className="h-8 border-border/40 bg-card font-mono text-xs"
                        value={line.debit}
                        onChange={(e) => updateJournalLine(index, "debit", e.target.value)}
                      />
                    </div>
                    <div className="col-span-2.5">
                      <Input
                        placeholder="Credit"
                        type="number"
                        step="0.01"
                        className="h-8 border-border/40 bg-card font-mono text-xs"
                        value={line.credit}
                        onChange={(e) => updateJournalLine(index, "credit", e.target.value)}
                      />
                    </div>
                    <div className="col-span-1 text-center">
                      {journalLines.length > 2 && (
                        <button
                          type="button"
                          className="font-bold text-destructive hover:underline"
                          onClick={() => removeJournalLine(index)}
                        >
                          ×
                        </button>
                      )}
                    </div>
                  </div>
                ))}
              </ScrollArea>
            </div>

            {/* Balancer Summary */}
            <div className="flex items-center justify-between rounded border bg-muted/20 p-3 text-xs">
              <div>
                Total Debits:{" "}
                <span className="font-mono font-bold">
                  {currencySymbol}
                  {totalDebits.toFixed(2)}
                </span>
              </div>
              <div>
                Total Credits:{" "}
                <span className="font-mono font-bold text-rose-600">
                  {currencySymbol}
                  {totalCredits.toFixed(2)}
                </span>
              </div>
              <div
                className={`rounded px-2 py-0.5 font-bold ${isBalanced ? "bg-emerald-500/10 text-emerald-600" : "bg-rose-500/10 text-rose-600"}`}
              >
                {isBalanced ? "Balanced" : "Unbalanced"}
              </div>
            </div>

            <DialogFooter>
              <Button type="button" variant="outline" onClick={() => setIsJournalModalOpen(false)}>
                Cancel
              </Button>
              <Button type="submit" disabled={!isBalanced}>
                Post Entry
              </Button>
            </DialogFooter>
          </form>
        </DialogContent>
      </Dialog>
    </div>
  );
}
