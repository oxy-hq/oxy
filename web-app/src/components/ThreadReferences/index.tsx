import { Reference } from "@/types/chat";
import { ReferenceItem } from "./ReferenceItem";

type ThreadReferencesProps = {
  references: Reference[];
};

const ThreadReferences = ({ references }: ThreadReferencesProps) => {
  return (
    <div className="gap-2 flex overflow-x-auto font-sans">
      {references.map((reference, index) => (
        <ReferenceItem key={index} reference={reference} />
      ))}
    </div>
  );
};

export default ThreadReferences;
