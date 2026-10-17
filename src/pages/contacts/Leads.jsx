import React, { useState, useEffect } from "react";
import { useNavigate } from "react-router-dom";
import { invoke } from "@tauri-apps/api/core";
import { useStore } from "@/hooks/use-store";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Card, CardContent } from "@/components/ui/card";
import { Dialog, DialogContent, DialogHeader, DialogTitle, DialogFooter } from "@/components/ui/dialog";
import { Label } from "@/components/ui/label";
import { HugeiconsIcon } from "@hugeicons/react";
import {
  Add01Icon,
  Search01Icon,
  Delete02Icon,
  Edit02Icon,
  UserGroupIcon,
  BookOpen01Icon,
  TargetIcon,
  ArrowLeft01Icon,
  ArrowRight01Icon
} from "@hugeicons/core-free-icons";

export default function Leads() {
  const navigate = useNavigate();
  const token = useStore((state) => state.token);
  const currencySymbol = useStore((state) => state.currencySymbol);
  const userRole = useStore((state) => state.user?.role);

  const isReadOnly = userRole === "accountant";
  const cannotDelete = userRole === "staff" || userRole === "accountant";

  // Data States
  const [leads, setLeads] = useState([]);
  const [loading, setLoading] = useState(true);

  // Search & Filter State
  const [searchQuery, setSearchQuery] = useState("");
  const [selectedStage, setSelectedStage] = useState("");

  // Pagination & Sorting State
  const [totalCount, setTotalCount] = useState(0);
  const [page, setPage] = useState(1);
  const [limit, setLimit] = useState(10);
  const [sortBy, setSortBy] = useState("name");
  const [sortOrder, setSortOrder] = useState("ASC");

  // Modals States
  const [isLeadModalOpen, setIsLeadModalOpen] = useState(false);
  const [editingLead, setEditingLead] = useState(null);
  const [leadForm, setLeadForm] = useState({
    name: "",
    contact_name: "",
    phone: "",
    email: "",
    stage: "PROSPECT",
    deal_value: 0,
    notes: ""
  });

  // Fetch Methods
  const fetchData = async () => {
    setLoading(true);
    try {
      const paginated = await invoke("list_leads_filtered_cmd", {
        token,
        search: searchQuery || null,
        stage: selectedStage || null,
        sortBy,
        sortOrder,
        page,
        limit,
      });
      setLeads(paginated.leads);
      setTotalCount(paginated.total_count);
    } catch (e) {
      console.error("Failed to load leads data:", e);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    fetchData();
  }, [token, selectedStage, sortBy, sortOrder, page, limit]);

  // Reset page to 1 when search or stage changes
  useEffect(() => {
    setPage(1);
    fetchData();
  }, [searchQuery, selectedStage]);

  // CRM Lead submit
  const handleLeadSubmit = async (e) => {
    e.preventDefault();
    try {
      if (editingLead) {
        await invoke("update_lead_cmd", {
          token,
          id: editingLead.id,
          name: leadForm.name,
          contactName: leadForm.contact_name || null,
          phone: leadForm.phone || null,
          email: leadForm.email || null,
          stage: leadForm.stage,
          dealValue: parseFloat(leadForm.deal_value) || 0,
          notes: leadForm.notes || null
        });
      } else {
        await invoke("create_lead_cmd", {
          token,
          name: leadForm.name,
          contactName: leadForm.contact_name || null,
          phone: leadForm.phone || null,
          email: leadForm.email || null,
          stage: leadForm.stage,
          dealValue: parseFloat(leadForm.deal_value) || 0,
          notes: leadForm.notes || null
        });
      }
      setIsLeadModalOpen(false);
      fetchData();
    } catch (e) {
      alert("Error saving lead: " + e);
    }
  };

  const handleLeadDelete = async (id) => {
    if (!window.confirm("Are you sure you want to delete this lead?")) return;
    try {
      await invoke("delete_lead_cmd", { token, id });
      fetchData();
    } catch (e) {
      alert("Error deleting lead: " + e);
    }
  };

  const openAddLead = () => {
    setEditingLead(null);
    setLeadForm({ name: "", contact_name: "", phone: "", email: "", stage: "PROSPECT", deal_value: 0, notes: "" });
    setIsLeadModalOpen(true);
  };

  const openEditLead = (l) => {
    setEditingLead(l);
    setLeadForm({
      name: l.name,
      contact_name: l.contact_name || "",
      phone: l.phone || "",
      email: l.email || "",
      stage: l.stage,
      deal_value: l.deal_value,
      notes: l.notes || ""
    });
    setIsLeadModalOpen(true);
  };

  const toggleSort = (field) => {
    if (sortBy === field) {
      setSortOrder(sortOrder === "ASC" ? "DESC" : "ASC");
    } else {
      setSortBy(field);
      setSortOrder("ASC");
    }
  };

  return (
    <div className="space-y-6 pb-8 font-sans">
      <div className="flex flex-col md:flex-row justify-between items-start md:items-center gap-4 border-b border-border/40 pb-4">
        <div>
          <h1 className="text-3xl font-bold tracking-tight">CRM & Contacts</h1>
          <p className="text-muted-foreground">Manage customer profiles, supplier balances, and track CRM deals pipeline.</p>
        </div>
        <div className="flex gap-2 print:hidden">
          <Button
            variant="outline"
            onClick={() => navigate("/contacts/customers")}
            className="flex items-center gap-1.5"
          >
            <HugeiconsIcon icon={UserGroupIcon} size={16} /> Customers CRM
          </Button>
          <Button
            variant="outline"
            onClick={() => navigate("/contacts/suppliers")}
            className="flex items-center gap-1.5"
          >
            <HugeiconsIcon icon={BookOpen01Icon} size={16} /> Suppliers Ledger
          </Button>
          <Button
            variant="default"
            onClick={() => navigate("/contacts/leads")}
            className="flex items-center gap-1.5"
          >
            <HugeiconsIcon icon={TargetIcon} size={16} /> CRM Leads
          </Button>
        </div>
      </div>

      <div className="space-y-4">
        <div className="flex flex-col sm:flex-row justify-between items-start sm:items-center gap-3">
          <div className="flex flex-1 gap-2 w-full sm:w-auto">
            <div className="relative flex-1 max-w-sm">
              <HugeiconsIcon icon={Search01Icon} size={18} strokeWidth={2} className="absolute left-3 top-2.5 text-muted-foreground" />
              <Input
                placeholder="Search leads..."
                value={searchQuery}
                onChange={(e) => setSearchQuery(e.target.value)}
                className="pl-9 bg-card border-border/40"
              />
            </div>
            <select
              value={selectedStage}
              onChange={(e) => setSelectedStage(e.target.value)}
              className="p-2 border rounded-md text-xs bg-card border-border/40 text-foreground max-w-xs focus:outline-none"
            >
              <option value="">All Stages</option>
              <option value="PROSPECT">Prospect</option>
              <option value="QUALIFIED">Qualified</option>
              <option value="PROPOSAL">Proposal</option>
              <option value="NEGOTIATING">Negotiating</option>
              <option value="WON">Won (Deal Closed)</option>
              <option value="LOST">Lost</option>
            </select>
          </div>
          {!isReadOnly && (
            <Button onClick={openAddLead} className="flex items-center gap-1.5">
              <HugeiconsIcon icon={Add01Icon} size={16} /> Add Lead
            </Button>
          )}
        </div>

        {loading ? (
          <div className="flex h-[30vh] items-center justify-center">
            <div className="h-8 w-8 animate-spin rounded-full border-4 border-primary border-t-transparent"></div>
          </div>
        ) : (
          <Card className="border-border/40 bg-card/65 backdrop-blur-sm">
            <CardContent className="p-0">
              <div className="overflow-x-auto">
                <table className="w-full text-sm text-left border-collapse">
                  <thead>
                    <tr className="border-b border-border/40 text-muted-foreground text-xs uppercase bg-muted/20">
                      <th className="p-4 font-medium cursor-pointer select-none" onClick={() => toggleSort("name")}>
                        Opportunity / Company {sortBy === "name" ? (sortOrder === "ASC" ? "▲" : "▼") : ""}
                      </th>
                      <th className="p-4 font-medium">Contact Person</th>
                      <th className="p-4 font-medium cursor-pointer select-none" onClick={() => toggleSort("stage")}>
                        Pipeline Stage {sortBy === "stage" ? (sortOrder === "ASC" ? "▲" : "▼") : ""}
                      </th>
                      <th className="p-4 font-medium text-right cursor-pointer select-none" onClick={() => toggleSort("deal_value")}>
                        Deal Value {sortBy === "deal_value" ? (sortOrder === "ASC" ? "▲" : "▼") : ""}
                      </th>
                      <th className="p-4 font-medium">Notes</th>
                      {!isReadOnly && <th className="p-4 font-medium text-right">Actions</th>}
                    </tr>
                  </thead>
                  <tbody className="divide-y divide-border/20">
                    {leads.map((l) => (
                      <tr key={l.id} className="hover:bg-muted/10">
                        <td className="p-4">
                          <div className="font-semibold">{l.name}</div>
                          <div className="text-[10px] text-muted-foreground">Phone: {l.phone || "-"}</div>
                        </td>
                        <td className="p-4 font-medium text-xs">
                          <div>{l.contact_name || "-"}</div>
                          <div className="text-[10px] text-gray-400">{l.email || ""}</div>
                        </td>
                        <td className="p-4">
                          <span className={`px-2 py-0.5 text-[10px] font-bold rounded-full ${
                            l.stage === "WON" ? "bg-emerald-500/10 text-emerald-600 border border-emerald-500/20" :
                            l.stage === "LOST" ? "bg-rose-500/10 text-rose-600 border border-rose-500/20" :
                            "bg-amber-500/10 text-amber-600 border border-amber-500/20"
                          }`}>
                            {l.stage}
                          </span>
                        </td>
                        <td className="p-4 text-right font-mono font-bold text-primary">{currencySymbol}{l.deal_value.toFixed(2)}</td>
                        <td className="p-4 text-xs text-muted-foreground truncate max-w-xs">{l.notes || "-"}</td>
                        {!isReadOnly && (
                          <td className="p-4 text-right">
                            <div className="flex justify-end gap-1.5">
                              <Button variant="ghost" size="icon" className="h-8 w-8 text-primary" onClick={() => openEditLead(l)}>
                                <HugeiconsIcon icon={Edit02Icon} size={14} />
                              </Button>
                              {!cannotDelete && (
                                <Button variant="ghost" size="icon" className="h-8 w-8 text-destructive" onClick={() => handleLeadDelete(l.id)}>
                                  <HugeiconsIcon icon={Delete02Icon} size={14} />
                                </Button>
                              )}
                            </div>
                          </td>
                        )}
                      </tr>
                    ))}
                    {leads.length === 0 && (
                      <tr>
                        <td colSpan={6} className="py-8 text-center text-muted-foreground text-xs">
                          No opportunities/leads registered.
                        </td>
                      </tr>
                    )}
                  </tbody>
                </table>
              </div>

              {/* Pagination controls */}
              <div className="p-4 border-t border-border/40 flex flex-col sm:flex-row justify-between items-center gap-4 text-xs text-muted-foreground print:hidden">
                <div className="flex items-center gap-2">
                  <span>Rows per page:</span>
                  <select
                    value={limit}
                    onChange={(e) => {
                      setLimit(parseInt(e.target.value));
                      setPage(1);
                    }}
                    className="p-1 border rounded bg-card border-border/40 text-foreground text-xs focus:outline-none"
                  >
                    <option value={5}>5</option>
                    <option value={10}>10</option>
                    <option value={20}>20</option>
                    <option value={50}>50</option>
                  </select>
                </div>
                <div className="flex items-center gap-1">
                  <Button
                    variant="outline"
                    size="icon"
                    onClick={() => setPage(Math.max(1, page - 1))}
                    disabled={page === 1}
                  >
                    <HugeiconsIcon icon={ArrowLeft01Icon} size={14} />
                  </Button>
                  <span className="px-2 font-medium">Page {page} of {Math.ceil(totalCount / limit) || 1}</span>
                  <Button
                    variant="outline"
                    size="icon"
                    onClick={() => setPage(Math.min(Math.ceil(totalCount / limit), page + 1))}
                    disabled={page >= Math.ceil(totalCount / limit)}
                  >
                    <HugeiconsIcon icon={ArrowRight01Icon} size={14} />
                  </Button>
                </div>
              </div>
            </CardContent>
          </Card>
        )}
      </div>

      {/* Lead Modal */}
      <Dialog open={isLeadModalOpen} onOpenChange={setIsLeadModalOpen}>
        <DialogContent className="sm:max-w-md font-sans">
          <DialogHeader>
            <DialogTitle>{editingLead ? "Edit Lead Detail" : "New CRM Opportunity"}</DialogTitle>
          </DialogHeader>
          <form onSubmit={handleLeadSubmit} className="space-y-4">
            <div className="space-y-2">
              <Label htmlFor="l_name">Opportunity / Company Name *</Label>
              <Input
                id="l_name"
                value={leadForm.name}
                onChange={(e) => setLeadForm({ ...leadForm, name: e.target.value })}
                required
              />
            </div>
            <div className="space-y-2">
              <Label htmlFor="l_contact">Contact Person Name</Label>
              <Input
                id="l_contact"
                value={leadForm.contact_name}
                onChange={(e) => setLeadForm({ ...leadForm, contact_name: e.target.value })}
              />
            </div>
            <div className="grid grid-cols-2 gap-4">
              <div className="space-y-2">
                <Label htmlFor="l_phone">Phone</Label>
                <Input
                  id="l_phone"
                  value={leadForm.phone}
                  onChange={(e) => setLeadForm({ ...leadForm, phone: e.target.value })}
                />
              </div>
              <div className="space-y-2">
                <Label htmlFor="l_email">Email</Label>
                <Input
                  id="l_email"
                  type="email"
                  value={leadForm.email}
                  onChange={(e) => setLeadForm({ ...leadForm, email: e.target.value })}
                />
              </div>
            </div>
            <div className="grid grid-cols-2 gap-4">
              <div className="space-y-2">
                <Label htmlFor="l_stage">Pipeline Stage *</Label>
                <select
                  id="l_stage"
                  className="w-full p-2 border rounded-md text-sm bg-card border-border/40 text-foreground"
                  value={leadForm.stage}
                  onChange={(e) => setLeadForm({ ...leadForm, stage: e.target.value })}
                  required
                >
                  <option value="PROSPECT">Prospect</option>
                  <option value="QUALIFIED">Qualified</option>
                  <option value="PROPOSAL">Proposal</option>
                  <option value="NEGOTIATING">Negotiating</option>
                  <option value="WON">Won (Deal Closed)</option>
                  <option value="LOST">Lost</option>
                </select>
              </div>
              <div className="space-y-2">
                <Label htmlFor="l_value">Estimated Deal Value *</Label>
                <Input
                  id="l_value"
                  type="number"
                  value={leadForm.deal_value}
                  onChange={(e) => setLeadForm({ ...leadForm, deal_value: e.target.value })}
                  required
                />
              </div>
            </div>
            <div className="space-y-2">
              <Label htmlFor="l_notes">Notes / Activities</Label>
              <Input
                id="l_notes"
                placeholder="Details of next follow-up"
                value={leadForm.notes}
                onChange={(e) => setLeadForm({ ...leadForm, notes: e.target.value })}
              />
            </div>
            <DialogFooter>
              <Button type="button" variant="outline" onClick={() => setIsLeadModalOpen(false)}>Cancel</Button>
              <Button type="submit">Save Opportunity</Button>
            </DialogFooter>
          </form>
        </DialogContent>
      </Dialog>
    </div>
  );
}
