import EditorPageWrapper from "../components/EditorPageWrapper";
import { useEditorContext } from "../contexts/useEditorContext";

const DefaultEditor = () => {
  const { pathb64, isReadOnly, gitEnabled } = useEditorContext();

  return (
    <EditorPageWrapper
      pathb64={pathb64}
      readOnly={isReadOnly}
      git={gitEnabled}
    />
  );
};
export default DefaultEditor;
