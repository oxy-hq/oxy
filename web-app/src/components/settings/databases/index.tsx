import PageWrapper from "../components/PageWrapper";
import { Separator } from "@/components/ui/shadcn/separator";
import DatabaseTable from "./DatabaseTable";
import { EmbeddingsManagement } from "./EmbeddingsManagement";

const DatabaseManagement = () => {
  return (
    <PageWrapper title="Database Management">
      <DatabaseTable />

      <Separator className="my-6" />

      <EmbeddingsManagement />
    </PageWrapper>
  );
};

export default DatabaseManagement;
