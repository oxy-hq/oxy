import { Reference } from "@/types/chat";
import { ReferenceItem } from "./ReferenceItem";

type ReferencesProps = {
  references: Reference[];
};

const References = ({ references }: ReferencesProps) => {
  return (
    <div className="gap-4 flex overflow-x-auto font-sans">
      {references.map((reference, index) => (
        <ReferenceItem key={index} reference={reference} />
      ))}
    </div>
  );
};

export default References;
