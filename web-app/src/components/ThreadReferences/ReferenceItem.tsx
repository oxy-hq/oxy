import { Reference, ReferenceType } from "@/types/chat";
import { QueryReference } from "./QueryReference";

type ReferenceProps = {
  reference: Reference;
};

export const ReferenceItem = ({ reference }: ReferenceProps) => {
  console.log("ReferenceItem", reference);
  if (reference.type === ReferenceType.SQLQuery) {
    return <QueryReference reference={reference} />;
  }
};
