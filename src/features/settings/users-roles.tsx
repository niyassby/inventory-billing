import { SelectField } from "@/components/ui/select-field";
import { useEffect, useState } from "react";
import type { Api } from "@inventory/shared";
import { KeyRound, Plus, Trash2 } from "lucide-react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Avatar } from "@/components/ui/avatar";
import { Checkbox } from "@/components/ui/checkbox";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Field, FieldLabel } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { confirm } from "@/components/app/dialogs";
import { RateInput } from "@/components/app/inputs";
import { PermissionGrid } from "./permission-grid";
import { QueryView, Section } from "@/components/app/page";
import { showDateTime } from "@/lib/format";
import { run } from "@/lib/run";
import { useAction, useCommand } from "@/lib/queries";
import { useCan, useSession } from "@/stores/session";

// ------------------------------------------------------------------------------------- users

type UserDraft = {
  id: string | null;
  name: string;
  email: string;
  phone: string;
  roleId: string;
  password: string;
  isActive: boolean;
};

function UserDialog({
  draft,
  roles,
  onClose,
}: {
  draft: UserDraft | null;
  roles: Api.RoleSummary[];
  onClose: () => void;
}) {
  const [d, setD] = useState<UserDraft | null>(draft);
  useEffect(() => setD(draft), [draft]);
  const create = useAction("users_create", {
    invalidates: ["users_list", "roles_list"],
    success: "User added",
    onSuccess: onClose,
  });
  const update = useAction("users_update", {
    invalidates: ["users_list", "roles_list"],
    success: "User saved",
    onSuccess: onClose,
  });
  if (!d)
    return (
      <Dialog open={false} onOpenChange={() => undefined}>
        <DialogContent />
      </Dialog>
    );
  const set = (change: Partial<UserDraft>) => setD((x) => (x ? { ...x, ...change } : x));
  return (
    <Dialog open onOpenChange={(o) => !o && onClose()}>
      <DialogContent>
        <form
          className="flex flex-col gap-4"
          onSubmit={(e) => {
            e.preventDefault();
            if (d.id)
              update.mutate({
                input: {
                  id: d.id,
                  name: d.name,
                  phone: d.phone || null,
                  roleId: d.roleId,
                  isActive: d.isActive,
                },
              });
            else
              create.mutate({
                input: {
                  name: d.name,
                  email: d.email,
                  phone: d.phone || null,
                  roleId: d.roleId,
                  temporaryPassword: d.password,
                },
              });
          }}
        >
          <DialogHeader>
            <DialogTitle>{d.id ? "Edit user" : "New user"}</DialogTitle>
            {!d.id && (
              <DialogDescription>
                They must choose their own password the first time they sign in.
              </DialogDescription>
            )}
          </DialogHeader>
          <Field>
            <FieldLabel htmlFor="u-name">Name</FieldLabel>
            <Input
              id="u-name"
              autoFocus
              required
              value={d.name}
              onChange={(e) => set({ name: e.target.value })}
            />
          </Field>
          {!d.id && (
            <Field>
              <FieldLabel htmlFor="u-email">E-mail (they sign in with it)</FieldLabel>
              <Input
                id="u-email"
                type="email"
                required
                value={d.email}
                onChange={(e) => set({ email: e.target.value })}
              />
            </Field>
          )}
          <div className="grid grid-cols-2 gap-3">
            <Field>
              <FieldLabel htmlFor="u-phone">Phone</FieldLabel>
              <Input
                id="u-phone"
                value={d.phone}
                onChange={(e) => set({ phone: e.target.value })}
              />
            </Field>
            <Field>
              <FieldLabel htmlFor="u-role">Role</FieldLabel>
              <SelectField
                id="u-role"
                className="w-full"
                value={d.roleId}
                onValueChange={(v) => set({ roleId: v })}
                options={[...roles.map((r) => ({ value: r.id, label: r.name }))]}
              />
            </Field>
          </div>
          {!d.id && (
            <Field>
              <FieldLabel htmlFor="u-pass">Temporary password</FieldLabel>
              <Input
                id="u-pass"
                type="password"
                autoComplete="new-password"
                required
                value={d.password}
                onChange={(e) => set({ password: e.target.value })}
              />
            </Field>
          )}
          {d.id && (
            <label className="flex items-center gap-2 text-sm">
              <Checkbox checked={d.isActive} onCheckedChange={(isActive) => set({ isActive })} />{" "}
              Can sign in
            </label>
          )}
          <DialogFooter>
            <Button type="button" variant="outline" onClick={onClose}>
              Cancel
            </Button>
            <Button
              type="submit"
              disabled={create.isPending || update.isPending || !d.name.trim() || !d.roleId}
            >
              Save
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}

function ResetDialog({ user, onClose }: { user: Api.UserSummary | null; onClose: () => void }) {
  const [password, setPassword] = useState("");
  useEffect(() => setPassword(""), [user]);
  const reset = useAction("users_reset_password", {
    invalidates: ["users_list"],
    success: "Password reset: they must choose a new one at next sign-in",
    onSuccess: onClose,
  });
  return (
    <Dialog open={user !== null} onOpenChange={(o) => !o && onClose()}>
      <DialogContent>
        <form
          className="flex flex-col gap-4"
          onSubmit={(e) => {
            e.preventDefault();
            if (user) reset.mutate({ input: { id: user.id, temporaryPassword: password } });
          }}
        >
          <DialogHeader>
            <DialogTitle>Reset password for {user?.name}</DialogTitle>
            <DialogDescription>
              Give them the temporary password in person. All their sign-ins end now.
            </DialogDescription>
          </DialogHeader>
          <Field>
            <FieldLabel htmlFor="r-pass">Temporary password</FieldLabel>
            <Input
              id="r-pass"
              type="password"
              autoComplete="new-password"
              autoFocus
              value={password}
              onChange={(e) => setPassword(e.target.value)}
            />
          </Field>
          <DialogFooter>
            <Button type="button" variant="outline" onClick={onClose}>
              Cancel
            </Button>
            <Button type="submit" disabled={!password || reset.isPending}>
              Reset
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}

export function UsersSection() {
  const me = useSession((s) => s.user);
  const canManage = useCan("users.manage");
  const users = useCommand("users_list", {}, { enabled: canManage });
  const roles = useCommand("roles_list", {}, { enabled: canManage });
  const [editing, setEditing] = useState<UserDraft | null>(null);
  const [resetting, setResetting] = useState<Api.UserSummary | null>(null);
  const remove = useAction("users_delete", {
    invalidates: ["users_list", "roles_list"],
    success: "User removed",
  });
  if (!canManage)
    return (
      <Section title="Users">
        <p className="text-sm text-muted-foreground">Only the owner manages users.</p>
      </Section>
    );
  return (
    <Section
      title="Users"
      actions={
        <Button
          size="sm"
          onClick={() =>
            setEditing({
              id: null,
              name: "",
              email: "",
              phone: "",
              roleId:
                roles.data?.find((r) => r.code === "cashier")?.id ?? roles.data?.[0]?.id ?? "",
              password: "",
              isActive: true,
            })
          }
        >
          <Plus /> New user
        </Button>
      }
    >
      <QueryView query={users}>
        {(list) => (
          <table className="w-full text-sm">
            <thead className="text-xs text-muted-foreground">
              <tr>
                <th className="py-1 text-left font-medium">Name</th>
                <th className="py-1 text-left font-medium">Role</th>
                <th className="py-1 text-left font-medium">Last sign-in</th>
                <th />
              </tr>
            </thead>
            <tbody>
              {list.map((u) => (
                <tr key={u.id} className="border-t border-border/60">
                  <td className="py-2">
                    <div className="flex items-center gap-3">
                      <Avatar name={u.name} />
                      <div>
                        <p className="font-medium">
                          {u.name} {u.id === me?.id && <span className="text-caption">(you)</span>}
                        </p>
                        <p className="text-caption">{u.email}</p>
                      </div>
                    </div>
                  </td>
                  <td>
                    <span className="flex flex-wrap items-center gap-1.5">
                      <Badge variant="default">{u.roleName}</Badge>
                      {!u.isActive && <Badge variant="destructive">Disabled</Badge>}
                      {u.mustChangePassword && <Badge variant="warning">New password due</Badge>}
                    </span>
                  </td>
                  <td className="text-muted-foreground">
                    {u.lastLoginAt ? showDateTime(u.lastLoginAt) : "never"}
                  </td>
                  <td className="text-right">
                    <Button
                      variant="ghost"
                      size="xs"
                      onClick={() =>
                        setEditing({
                          id: u.id,
                          name: u.name,
                          email: u.email,
                          phone: u.phone ?? "",
                          roleId: u.roleId,
                          password: "",
                          isActive: u.isActive,
                        })
                      }
                    >
                      Edit
                    </Button>
                    <Button
                      variant="ghost"
                      size="icon-sm"
                      aria-label={`Reset password of ${u.name}`}
                      onClick={() => setResetting(u)}
                    >
                      <KeyRound />
                    </Button>
                    {u.id !== me?.id && (
                      <Button
                        variant="ghost"
                        size="icon-sm"
                        aria-label={`Remove ${u.name}`}
                        onClick={run(async () => {
                          if (
                            (await confirm({
                              title: `Remove ${u.name}?`,
                              description: "Their past work stays on record under their name.",
                              confirmLabel: "Remove",
                              destructive: true,
                            })) !== false
                          )
                            remove.mutate({ input: { id: u.id } });
                        })}
                      >
                        <Trash2 />
                      </Button>
                    )}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </QueryView>
      <UserDialog draft={editing} roles={roles.data ?? []} onClose={() => setEditing(null)} />
      <ResetDialog user={resetting} onClose={() => setResetting(null)} />
    </Section>
  );
}

// ------------------------------------------------------------------------------------- roles

type RoleDraft = {
  id: string | null;
  code: string;
  name: string;
  maxDiscountRate: number | null;
  permissions: Set<string>;
  isSystem: boolean;
};

function slug(name: string): string {
  return name
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "_")
    .replace(/^_|_$/g, "");
}

export function RolesSection() {
  const canManage = useCan("users.manage");
  const roles = useCommand("roles_list", {}, { enabled: canManage });
  const permissions = useCommand("permissions_list", {}, { enabled: canManage });
  const [editing, setEditing] = useState<RoleDraft | null>(null);
  const save = useAction("roles_save", {
    invalidates: ["roles_list", "users_list"],
    success: "Role saved",
    onSuccess: () => setEditing(null),
  });
  const remove = useAction("roles_delete", {
    invalidates: ["roles_list"],
    success: "Role deleted",
  });
  if (!canManage) return null;
  return (
    <Section
      title="Roles and permissions"
      actions={
        <Button
          size="sm"
          variant="outline"
          onClick={() =>
            setEditing({
              id: null,
              code: "",
              name: "",
              maxDiscountRate: 0,
              permissions: new Set([
                "dashboard.view",
                "products.view",
                "sales.create",
                "sales.view",
              ]),
              isSystem: false,
            })
          }
        >
          <Plus /> New role
        </Button>
      }
    >
      <QueryView query={roles}>
        {(list) => (
          <ul className="divide-y divide-border/40 text-sm">
            {list.map((r) => (
              <li key={r.id} className="flex items-center gap-3 py-2">
                <div className="flex-1">
                  <p className="font-medium">
                    {r.name} {r.isSystem && <Badge variant="outline">Built in</Badge>}
                  </p>
                  <p className="text-xs text-muted-foreground">
                    {r.userCount} users · {r.permissions.length} permissions · discounts up to{" "}
                    {r.maxDiscountRate / 10_000}%
                  </p>
                </div>
                {r.code !== "owner" && (
                  <Button
                    variant="ghost"
                    size="xs"
                    onClick={() =>
                      setEditing({
                        id: r.id,
                        code: r.code,
                        name: r.name,
                        maxDiscountRate: r.maxDiscountRate,
                        permissions: new Set(r.permissions),
                        isSystem: r.isSystem,
                      })
                    }
                  >
                    Edit
                  </Button>
                )}
                {!r.isSystem && (
                  <Button
                    variant="ghost"
                    size="icon-sm"
                    aria-label={`Delete ${r.name}`}
                    onClick={run(async () => {
                      if (
                        (await confirm({
                          title: `Delete the role ${r.name}?`,
                          description: "Roles with users can't be deleted.",
                          confirmLabel: "Delete",
                          destructive: true,
                        })) !== false
                      )
                        remove.mutate({ input: { id: r.id } });
                    })}
                  >
                    <Trash2 />
                  </Button>
                )}
              </li>
            ))}
          </ul>
        )}
      </QueryView>
      <Dialog open={editing !== null} onOpenChange={(o) => !o && setEditing(null)}>
        <DialogContent className="sm:max-w-3xl">
          {editing && (
            <form
              className="flex flex-col gap-4"
              onSubmit={(e) => {
                e.preventDefault();
                save.mutate({
                  input: {
                    id: editing.id,
                    code: editing.code || slug(editing.name),
                    name: editing.name,
                    maxDiscountRate: editing.maxDiscountRate ?? 0,
                    permissions: [...editing.permissions],
                  },
                });
              }}
            >
              <DialogHeader>
                <DialogTitle>{editing.id ? `Edit ${editing.name}` : "New role"}</DialogTitle>
                <DialogDescription>
                  The backend enforces these. Anything not ticked is refused, even if a screen shows
                  it.
                </DialogDescription>
              </DialogHeader>
              <div className="grid grid-cols-2 gap-3">
                <Field>
                  <FieldLabel htmlFor="role-name">Name</FieldLabel>
                  <Input
                    id="role-name"
                    autoFocus
                    required
                    value={editing.name}
                    onChange={(e) => setEditing({ ...editing, name: e.target.value })}
                  />
                </Field>
                <Field>
                  <FieldLabel htmlFor="role-disc">Biggest discount without approval</FieldLabel>
                  <RateInput
                    id="role-disc"
                    value={editing.maxDiscountRate}
                    onValueChange={(maxDiscountRate) => setEditing({ ...editing, maxDiscountRate })}
                  />
                </Field>
              </div>
              <PermissionGrid
                permissions={permissions.data ?? []}
                selected={editing.permissions}
                onChange={(next) => setEditing({ ...editing, permissions: next })}
              />
              <DialogFooter>
                <Button type="button" variant="outline" onClick={() => setEditing(null)}>
                  Cancel
                </Button>
                <Button type="submit" disabled={!editing.name.trim() || save.isPending}>
                  Save role
                </Button>
              </DialogFooter>
            </form>
          )}
        </DialogContent>
      </Dialog>
    </Section>
  );
}
