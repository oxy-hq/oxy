import { useParams } from "react-router-dom";
import { EditorRouter } from "./EditorRouter";
import { EditorProvider } from "./contexts/EditorContext";

const EditorPage = () => {
  const { pathb64 } = useParams();

  if (!pathb64) {
    return null;
  }

  return (
    <EditorProvider key={pathb64} pathb64={pathb64}>
      <EditorRouter />
    </EditorProvider>
  );
};

export default EditorPage;
