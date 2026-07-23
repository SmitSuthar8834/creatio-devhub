import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import LookupMigration from "./LookupMigration";
import ObjectMigration from "./ObjectMigration";

export default function MigrationPage({ onShowJobs }: { onShowJobs: () => void }) {
  return (
    <div className="mx-auto grid w-full max-w-6xl gap-4 p-6">
      <div>
        <h1 className="text-xl font-semibold tracking-tight">Migrate data</h1>
        <p className="mt-1 text-sm text-muted-foreground">
          Move reference and record data between environments.
        </p>
      </div>

      <Tabs defaultValue="lookups" className="gap-4">
        <TabsList>
          <TabsTrigger value="lookups">Lookups</TabsTrigger>
          <TabsTrigger value="objects">Objects</TabsTrigger>
        </TabsList>
        <TabsContent value="lookups">
          <LookupMigration onShowJobs={onShowJobs} />
        </TabsContent>
        <TabsContent value="objects">
          <ObjectMigration />
        </TabsContent>
      </Tabs>
    </div>
  );
}
