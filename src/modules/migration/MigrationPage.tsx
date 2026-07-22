import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import LookupMigration from "./LookupMigration";
import ContentMigration from "./ContentMigration";

export default function MigrationPage({ onShowJobs }: { onShowJobs: () => void }) {
  return (
    <Tabs defaultValue="lookups" className="mx-auto w-full max-w-6xl gap-4 p-6">
      <TabsList>
        <TabsTrigger value="lookups">Lookups</TabsTrigger>
        <TabsTrigger value="content">Marketing content</TabsTrigger>
      </TabsList>
      <TabsContent value="lookups"><LookupMigration onShowJobs={onShowJobs} /></TabsContent>
      <TabsContent value="content"><ContentMigration /></TabsContent>
    </Tabs>
  );
}
