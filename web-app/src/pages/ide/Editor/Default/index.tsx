import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import EditorPageWrapper from "../components/EditorPageWrapper";

const DefaultEditor = ({ pathb64 }: { pathb64: string }) => {
  const { isReadOnly } = useCurrentProjectBranch();
  return <EditorPageWrapper pathb64={pathb64} readOnly={isReadOnly} />;
};
export default DefaultEditor;
