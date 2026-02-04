import type { Reference } from "@/types/chat";
import { ReferenceItem } from "./ReferenceItem";

type ThreadReferencesProps = {
  references: Reference[];
  prompt?: string;
};

const ThreadReferences = ({ references, prompt }: ThreadReferencesProps) => {
  return (
    <div className='flex flex-wrap gap-2 font-sans'>
      {references.map((reference, index) => (
        <ReferenceItem key={index} reference={reference} prompt={prompt} />
      ))}
    </div>
  );
};

export default ThreadReferences;
