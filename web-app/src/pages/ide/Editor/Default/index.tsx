import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import EditorPageWrapper from "../components/EditorPageWrapper";

const DefaultEditor = ({ pathb64 }: { pathb64: string }) => {
  const { isReadOnly, gitEnabled } = useCurrentProjectBranch();
  return (
    <EditorPageWrapper
      pathb64={pathb64}
      readOnly={!!isReadOnly}
      git={gitEnabled}
    />
  );
};
export default DefaultEditor;
