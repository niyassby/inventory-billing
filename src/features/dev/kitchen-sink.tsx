import { useState } from "react";
import type { ReactNode } from "react";
import { toast } from "sonner";
import { Download, Plus, Search, Trash2 } from "lucide-react";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Avatar } from "@/components/ui/avatar";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Checkbox } from "@/components/ui/checkbox";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from "@/components/ui/dialog";
import { Field, FieldDescription, FieldError, FieldLabel } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { Kbd } from "@/components/ui/kbd";
import { DatePicker } from "@/components/ui/date-picker";
import { SelectField } from "@/components/ui/select-field";
import { Skeleton } from "@/components/ui/skeleton";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { Textarea } from "@/components/ui/textarea";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { Page, Section } from "@/components/app/page";

const SWATCHES = [
  ["background", "bg-background"],
  ["card", "bg-card"],
  ["raised", "bg-raised"],
  ["muted", "bg-muted"],
  ["accent", "bg-accent"],
  ["primary", "bg-primary"],
  ["primary-soft", "bg-primary-soft"],
  ["success", "bg-success"],
  ["success-soft", "bg-success-soft"],
  ["warning", "bg-warning"],
  ["warning-soft", "bg-warning-soft"],
  ["danger", "bg-danger"],
  ["danger-soft", "bg-danger-soft"],
  ["info", "bg-info"],
  ["info-soft", "bg-info-soft"],
  ["chart-1", "bg-chart-1"],
  ["chart-2", "bg-chart-2"],
  ["chart-3", "bg-chart-3"],
  ["chart-4", "bg-chart-4"],
  ["chart-5", "bg-chart-5"],
] as const;

function Row({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="flex flex-wrap items-center gap-3">
      <span className="w-28 shrink-0 text-caption">{label}</span>
      {children}
    </div>
  );
}

/** Development only (`/dev/ui`): every primitive, in the current theme. Not part of the packaged app. */
export default function KitchenSink() {
  const [category, setCategory] = useState("");
  const [day, setDay] = useState("2026-10-20");
  return (
    <Page
      title="Design system"
      description="Every primitive in the current theme. Use the theme button in the top bar to compare light and dark."
    >
      <Section title="Colour tokens">
        <div className="grid grid-cols-2 gap-3 sm:grid-cols-4 lg:grid-cols-5">
          {SWATCHES.map(([name, cls]) => (
            <div key={name} className="flex items-center gap-2">
              <span className={`size-8 rounded-md border border-border ${cls}`} />
              <span className="text-caption">{name}</span>
            </div>
          ))}
        </div>
      </Section>

      <Section title="Type scale">
        <div className="flex flex-col gap-2">
          <p className="text-display">₹12,450.00 display</p>
          <p className="text-page-title">Page title</p>
          <p className="text-section-title">Section title</p>
          <p className="text-sm">Body text, 14 px: the default for forms and descriptions.</p>
          <p className="text-table">Table text, 13 px</p>
          <p className="text-caption">Caption, 12 px muted</p>
          <p className="num text-sm">1,234.50 · 0.250 · 18.0% (tabular figures)</p>
        </div>
      </Section>

      <Section title="Buttons">
        <div className="flex flex-col gap-3">
          <Row label="variants">
            <Button>Primary</Button>
            <Button variant="soft">Soft</Button>
            <Button variant="outline">Outline</Button>
            <Button variant="secondary">Secondary</Button>
            <Button variant="ghost">Ghost</Button>
            <Button variant="destructive">Destructive</Button>
            <Button variant="link">Link</Button>
            <Button disabled>Disabled</Button>
          </Row>
          <Row label="sizes">
            <Button size="xs">xs</Button>
            <Button size="sm">sm</Button>
            <Button>default</Button>
            <Button size="lg">lg</Button>
            <Button size="xl">xl (till)</Button>
          </Row>
          <Row label="with icons">
            <Button>
              <Plus /> New product
            </Button>
            <Button variant="outline">
              <Download /> Export
            </Button>
            <Button variant="outline" size="icon" aria-label="Search">
              <Search />
            </Button>
            <Button variant="ghost" size="icon-sm" aria-label="Delete">
              <Trash2 />
            </Button>
          </Row>
        </div>
      </Section>

      <Section title="Badges">
        <div className="flex flex-wrap gap-2">
          <Badge>Brand</Badge>
          <Badge variant="secondary">Neutral</Badge>
          <Badge variant="success">Paid</Badge>
          <Badge variant="warning">Due</Badge>
          <Badge variant="destructive">Overdue</Badge>
          <Badge variant="info">Info</Badge>
          <Badge variant="outline">Outline</Badge>
        </div>
      </Section>

      <Section title="Form controls">
        <div className="grid gap-4 md:grid-cols-2">
          <Field>
            <FieldLabel htmlFor="ks-name">Name</FieldLabel>
            <Input id="ks-name" placeholder="Item name" />
            <FieldDescription>Shown on bills.</FieldDescription>
          </Field>
          <Field>
            <FieldLabel htmlFor="ks-bad">With an error</FieldLabel>
            <Input id="ks-bad" aria-invalid defaultValue="abc" />
            <FieldError>Enter a number.</FieldError>
          </Field>
          <Field>
            <FieldLabel htmlFor="ks-off">Disabled</FieldLabel>
            <Input id="ks-off" disabled defaultValue="Read only" />
          </Field>
          <Field>
            <FieldLabel htmlFor="ks-sel">Select</FieldLabel>
            <SelectField
              id="ks-sel"
              value={category}
              onValueChange={setCategory}
              options={[
                { value: "", label: "All categories" },
                { value: "grocery", label: "Grocery" },
                { value: "stationery", label: "Stationery" },
              ]}
            />
          </Field>
          <Field>
            <FieldLabel htmlFor="ks-date">Date</FieldLabel>
            <DatePicker id="ks-date" value={day} onChange={setDay} clearable />
          </Field>
          <Field>
            <FieldLabel htmlFor="ks-date-bad">Date with an error</FieldLabel>
            <DatePicker id="ks-date-bad" value="" onChange={setDay} invalid />
          </Field>
          <Field className="md:col-span-2">
            <FieldLabel htmlFor="ks-note">Note</FieldLabel>
            <Textarea id="ks-note" placeholder="Optional" />
          </Field>
          <div className="flex items-center gap-4">
            <label className="flex items-center gap-2 text-sm">
              <Checkbox defaultChecked /> Low stock only
            </label>
            <label className="flex items-center gap-2 text-sm">
              <Checkbox /> Show inactive
            </label>
            <span className="text-sm">
              Shortcut <Kbd>F10</Kbd>
            </span>
          </div>
        </div>
      </Section>

      <Section title="Notices">
        <div className="flex flex-col gap-2">
          <Alert variant="info">
            <AlertTitle>Working offline</AlertTitle>
            <AlertDescription>Sales are saved here and sent to the hub later.</AlertDescription>
          </Alert>
          <Alert variant="success">
            <AlertTitle>Backup finished</AlertTitle>
          </Alert>
          <Alert variant="warning">
            <AlertTitle>3 items are low on stock</AlertTitle>
          </Alert>
          <Alert variant="danger">
            <AlertTitle>Could not save</AlertTitle>
            <AlertDescription>The hub did not answer. Try again.</AlertDescription>
          </Alert>
        </div>
      </Section>

      <div className="grid gap-4 md:grid-cols-2">
        <Card>
          <CardHeader>
            <CardTitle>Card</CardTitle>
            <CardDescription>Surface with a subtle border.</CardDescription>
          </CardHeader>
          <CardContent className="flex items-center gap-3">
            <Avatar name="Basmati Rice" />
            <Avatar name="Steel Tumbler" />
            <Avatar name="Notebook A5" size="lg" />
            <Avatar name="Tea" size="sm" />
          </CardContent>
        </Card>
        <Section title="Loading">
          <div className="flex flex-col gap-2">
            <Skeleton className="h-9 w-full" />
            <Skeleton className="h-9 w-4/5" />
            <Skeleton className="h-9 w-3/5" />
          </div>
        </Section>
      </div>

      <Section title="Tabs, tooltip, dialog, toast">
        <div className="flex flex-col gap-4">
          <Tabs defaultValue="a">
            <TabsList>
              <TabsTrigger value="a">Overview</TabsTrigger>
              <TabsTrigger value="b">Ledger</TabsTrigger>
              <TabsTrigger value="c">History</TabsTrigger>
            </TabsList>
            <TabsContent value="a" className="pt-3">
              Tab content.
            </TabsContent>
          </Tabs>
          <div className="flex flex-wrap gap-2">
            <Tooltip>
              <TooltipTrigger render={<Button variant="outline" />}>Hover me</TooltipTrigger>
              <TooltipContent>A tooltip</TooltipContent>
            </Tooltip>
            <Dialog>
              <DialogTrigger render={<Button variant="outline" />}>Open dialog</DialogTrigger>
              <DialogContent>
                <DialogHeader>
                  <DialogTitle>Delete this item?</DialogTitle>
                  <DialogDescription>It can't be undone.</DialogDescription>
                </DialogHeader>
                <div className="grid gap-3 sm:grid-cols-2">
                  <SelectField
                    aria-label="Reason"
                    value={category}
                    onValueChange={setCategory}
                    options={[
                      { value: "", label: "Choose a reason" },
                      { value: "damaged", label: "Damaged" },
                      { value: "expired", label: "Expired" },
                    ]}
                  />
                  <DatePicker aria-label="Date in a dialog" value={day} onChange={setDay} />
                </div>
                <DialogFooter>
                  <Button variant="outline">Cancel</Button>
                  <Button variant="destructive">Delete</Button>
                </DialogFooter>
              </DialogContent>
            </Dialog>
            <Button variant="outline" onClick={() => toast.success("Saved")}>
              Success toast
            </Button>
            <Button variant="outline" onClick={() => toast.error("Could not save")}>
              Error toast
            </Button>
          </div>
        </div>
      </Section>

      <Section title="Table">
        <Table>
          <TableHeader>
            <TableRow>
              <TableHead>Item</TableHead>
              <TableHead className="text-right">Stock</TableHead>
              <TableHead className="text-right">Price</TableHead>
              <TableHead>Status</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            <TableRow>
              <TableCell>Basmati Rice 5kg</TableCell>
              <TableCell className="text-right num">36</TableCell>
              <TableCell className="text-right num">₹620.00</TableCell>
              <TableCell>
                <Badge variant="success">In stock</Badge>
              </TableCell>
            </TableRow>
            <TableRow>
              <TableCell>Toor Dal 1kg</TableCell>
              <TableCell className="text-right num">3</TableCell>
              <TableCell className="text-right num">₹165.00</TableCell>
              <TableCell>
                <Badge variant="warning">Low stock</Badge>
              </TableCell>
            </TableRow>
            <TableRow>
              <TableCell>Steel Tumbler</TableCell>
              <TableCell className="text-right num">0</TableCell>
              <TableCell className="text-right num">₹90.00</TableCell>
              <TableCell>
                <Badge variant="destructive">Out of stock</Badge>
              </TableCell>
            </TableRow>
          </TableBody>
        </Table>
      </Section>
    </Page>
  );
}
