import LookupMigration from "./LookupMigration";

export default function MigrationPage({ onShowJobs }: { onShowJobs: () => void }) {
  return (
    <div className="mx-auto w-full max-w-6xl p-6">
      <LookupMigration onShowJobs={onShowJobs} />
    </div>
  );
}
