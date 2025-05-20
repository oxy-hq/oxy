import { Reference } from "@/types/chat";
import { ReferenceItem } from "./ReferenceItem";

type ThreadReferencesProps = {
  references: Reference[];
  prompt?: string;
};

const ThreadReferences = ({ references, prompt }: ThreadReferencesProps) => {
  return (
    <div className="gap-2 flex overflow-x-auto font-sans">
      {references.map((reference, index) => (
        <ReferenceItem key={index} reference={reference} prompt={prompt} />
      ))}
    </div>
  );
};

export default ThreadReferences;
