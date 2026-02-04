import { type Reference, ReferenceType } from "@/types/chat";
import { QueryReference } from "./QueryReference";

type ReferenceProps = {
  reference: Reference;
  prompt?: string;
};

export const ReferenceItem = ({ reference, prompt }: ReferenceProps) => {
  if (reference.type === ReferenceType.SQLQuery) {
    return <QueryReference reference={reference} prompt={prompt} />;
  }
};
