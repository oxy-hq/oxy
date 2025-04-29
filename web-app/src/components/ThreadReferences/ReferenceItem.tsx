import { Reference, ReferenceType } from "@/types/chat";
import { QueryReference } from "./QueryReference";

type ReferenceProps = {
  reference: Reference;
};

export const ReferenceItem = ({ reference }: ReferenceProps) => {
  if (reference.type === ReferenceType.SQLQuery) {
    return <QueryReference reference={reference} />;
  }
};
