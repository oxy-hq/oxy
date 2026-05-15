import type { Reference } from "@/types/chat";
import { ReferenceItem } from "./ReferenceItem";

type ThreadReferencesProps = {
  references: Reference[];
};

const ThreadReferences = ({ references }: ThreadReferencesProps) => {
  return (
    <div className='flex flex-wrap gap-2 font-sans'>
      {references.map((reference, index) => (
        <ReferenceItem key={index} reference={reference} />
      ))}
    </div>
  );
};

export default ThreadReferences;
