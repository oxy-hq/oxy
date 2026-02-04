import { useParams } from "react-router-dom";
import { EditorProvider } from "./contexts/EditorContext";
import { EditorRouter } from "./EditorRouter";

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
