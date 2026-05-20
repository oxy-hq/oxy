import { type Reference, ReferenceType } from "@/types/chat";
import { ReferenceItem } from "./ReferenceItem";

type ThreadReferencesProps = {
  references: Reference[];
};

const referenceKey = (r: Reference): string =>
  r.type === ReferenceType.SQLQuery ? `sql:${r.database}:${r.sql_query}` : `app:${r.file_path}`;

const ThreadReferences = ({ references }: ThreadReferencesProps) => {
  return (
    <div className='flex flex-wrap gap-2 font-sans'>
      {references.map((reference) => (
        <ReferenceItem key={referenceKey(reference)} reference={reference} />
      ))}
    </div>
  );
};

export default ThreadReferences;
