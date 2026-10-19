import { Page, EmptyState } from "@/components/app/page";

/** Modules rebuilt in Phase 6 (reports, accounts, settings). */
export default function ComingSoon({ title }: { title: string }) {
  return (
    <Page title={title}>
      <EmptyState
        title="Coming in the next update"
        description={
          <>
            {title} are being rebuilt on the new backend. Your existing data is safe: it was copied
            into the new database, and the original file from the previous version is kept
            unchanged.
          </>
        }
      />
    </Page>
  );
}
