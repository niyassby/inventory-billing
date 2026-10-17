import React, { useState, useEffect } from "react";
import { useNavigate } from "react-router-dom";
import { invoke } from "@tauri-apps/api/core";
import { useStore } from "@/hooks/use-store";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogFooter,
} from "@/components/ui/dialog";
import { Label } from "@/components/ui/label";
import { HugeiconsIcon } from "@hugeicons/react";
import {
  Add01Icon,
  Delete02Icon,
  Edit02Icon,
  UserIcon,
  Database02Icon,
  Configuration02Icon,
  BookOpen01Icon,
} from "@hugeicons/core-free-icons";

export default function Users() {
  const navigate = useNavigate();
  const token = useStore((state) => state.token);
  const user = useStore((state) => state.user);
  const isAdmin = user?.role === "admin";

  // Data States
  const [usersList, setUsersList] = useState([]);
  const [loading, setLoading] = useState(true);

  // User Modals States
  const [isUserModalOpen, setIsUserModalOpen] = useState(false);
  const [editingUser, setEditingUser] = useState(null);
  const [userForm, setUserForm] = useState({ name: "", email: "", password: "", role: "staff" });

  // Fetch Users
  const fetchUsers = async () => {
    if (!isAdmin) return;
    setLoading(true);
    try {
      const list = await invoke("list_users_cmd", { token });
      setUsersList(list);
    } catch (e) {
      console.error("Failed to load user list:", e);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    fetchUsers();
  }, [token]);

  // User submit CRUD
  const handleUserSubmit = async (e) => {
    e.preventDefault();
    try {
      if (editingUser) {
        await invoke("update_user_cmd", {
          token,
          id: editingUser.id,
          name: userForm.name,
          email: userForm.email,
          role: userForm.role,
        });
      } else {
        await invoke("create_user_cmd", {
          token,
          name: userForm.name,
          email: userForm.email,
          passwordHash: userForm.password,
          role: userForm.role,
        });
      }
      setIsUserModalOpen(false);
      setUserForm({ name: "", email: "", password: "", role: "staff" });
      fetchUsers();
    } catch (e) {
      alert("Error saving user: " + e);
    }
  };

  const handleUserDelete = async (id) => {
    if (id === user.id) return alert("You cannot delete your own active session account!");
    if (!window.confirm("Are you sure you want to delete this user?")) return;
    try {
      await invoke("delete_user_cmd", { token, id });
      fetchUsers();
    } catch (e) {
      alert("Error deleting user: " + e);
    }
  };

  const openAddUser = () => {
    setEditingUser(null);
    setUserForm({ name: "", email: "", password: "", role: "staff" });
    setIsUserModalOpen(true);
  };

  const openEditUser = (usr) => {
    setEditingUser(usr);
    setUserForm({ name: usr.name, email: usr.email, password: "", role: usr.role });
    setIsUserModalOpen(true);
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
            variant="default"
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

      <div className="space-y-4">
        {!isAdmin ? (
          <Card className="max-w-lg border-rose-500/30 bg-rose-500/5">
            <CardHeader>
              <CardTitle className="text-rose-600">Restricted Tab Access</CardTitle>
              <CardDescription>
                Your account does not possess Super Administrator credentials. Please contact your
                manager to modify system accounts.
              </CardDescription>
            </CardHeader>
          </Card>
        ) : (
          <>
            <div className="flex items-center justify-between">
              <h3 className="text-lg font-semibold">System User Accounts</h3>
              <Button onClick={openAddUser} className="flex items-center gap-1.5">
                <HugeiconsIcon icon={Add01Icon} size={16} /> Create User Account
              </Button>
            </div>

            {loading ? (
              <div className="flex h-20 items-center justify-center">
                <div className="h-6 w-6 animate-spin rounded-full border-2 border-primary border-t-transparent"></div>
              </div>
            ) : (
              <Card className="border-border/40 bg-card/65 backdrop-blur-sm">
                <CardContent className="p-0">
                  <div className="overflow-x-auto">
                    <table className="w-full border-collapse text-left text-sm">
                      <thead>
                        <tr className="border-b border-border/40 bg-muted/20 text-xs text-muted-foreground uppercase">
                          <th className="p-4 font-medium">User Name</th>
                          <th className="p-4 font-medium">Email Address</th>
                          <th className="p-4 font-medium">System Role</th>
                          <th className="p-4 text-right font-medium">Actions</th>
                        </tr>
                      </thead>
                      <tbody className="divide-y divide-border/20">
                        {usersList.map((usr) => (
                          <tr key={usr.id} className="hover:bg-muted/10">
                            <td className="p-4 font-semibold">{usr.name}</td>
                            <td className="p-4 font-mono text-xs">{usr.email}</td>
                            <td className="p-4">
                              <span className="rounded-full border border-primary/20 bg-primary/10 px-2 py-0.5 text-[10px] font-semibold text-primary uppercase">
                                {usr.role}
                              </span>
                            </td>
                            <td className="p-4 text-right">
                              <div className="flex justify-end gap-1.5">
                                <Button
                                  variant="ghost"
                                  size="icon"
                                  className="h-8 w-8 text-primary"
                                  onClick={() => openEditUser(usr)}
                                >
                                  <HugeiconsIcon icon={Edit02Icon} size={14} />
                                </Button>
                                <Button
                                  variant="ghost"
                                  size="icon"
                                  className="h-8 w-8 text-destructive"
                                  onClick={() => handleUserDelete(usr.id)}
                                >
                                  <HugeiconsIcon icon={Delete02Icon} size={14} />
                                </Button>
                              </div>
                            </td>
                          </tr>
                        ))}
                      </tbody>
                    </table>
                  </div>
                </CardContent>
              </Card>
            )}
          </>
        )}
      </div>

      {/* Create / Edit User Account Modal Dialog */}
      <Dialog open={isUserModalOpen} onOpenChange={setIsUserModalOpen}>
        <DialogContent className="font-sans sm:max-w-md">
          <DialogHeader>
            <DialogTitle>{editingUser ? "Update User" : "Create User Account"}</DialogTitle>
          </DialogHeader>
          <form onSubmit={handleUserSubmit} className="space-y-4">
            <div className="space-y-2">
              <Label htmlFor="u_name">Full Name *</Label>
              <Input
                id="u_name"
                value={userForm.name}
                onChange={(e) => setUserForm({ ...userForm, name: e.target.value })}
                required
              />
            </div>
            <div className="space-y-2">
              <Label htmlFor="u_email">Email Address *</Label>
              <Input
                id="u_email"
                type="email"
                value={userForm.email}
                onChange={(e) => setUserForm({ ...userForm, email: e.target.value })}
                required
              />
            </div>
            {!editingUser && (
              <div className="space-y-2">
                <Label htmlFor="u_pwd">Initial Password *</Label>
                <Input
                  id="u_pwd"
                  type="password"
                  value={userForm.password}
                  onChange={(e) => setUserForm({ ...userForm, password: e.target.value })}
                  required
                />
              </div>
            )}
            <div className="space-y-2">
              <Label htmlFor="u_role">Access Role *</Label>
              <select
                id="u_role"
                className="w-full rounded-md border border-border/40 bg-card p-2 text-sm text-foreground"
                value={userForm.role}
                onChange={(e) => setUserForm({ ...userForm, role: e.target.value })}
                required
              >
                <option value="admin">Administrator (Full Rights)</option>
                <option value="manager">Manager (Inventory, CRM & Reports)</option>
                <option value="accountant">Accountant (General Ledger, Taxes & Reports)</option>
                <option value="staff">Staff Cashier (POS & Sales only)</option>
              </select>
            </div>
            <DialogFooter>
              <Button type="button" variant="outline" onClick={() => setIsUserModalOpen(false)}>
                Cancel
              </Button>
              <Button type="submit">Post Account Details</Button>
            </DialogFooter>
          </form>
        </DialogContent>
      </Dialog>
    </div>
  );
}
