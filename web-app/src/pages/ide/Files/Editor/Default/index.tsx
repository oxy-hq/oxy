import EditorPageWrapper from "../components/EditorPageWrapper";
import { useEditorContext } from "../contexts/useEditorContext";

const DefaultEditor = () => {
  const { pathb64, gitEnabled } = useEditorContext();

  return <EditorPageWrapper pathb64={pathb64} git={gitEnabled} />;
};
export default DefaultEditor;
