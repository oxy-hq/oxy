import { useState } from "react";
import EditorPageWrapper from "../components/EditorPageWrapper";
import AppPreview from "@/components/AppPreview";
import { useQueryClient } from "@tanstack/react-query";
import queryKeys from "@/hooks/api/queryKey";
import { randomKey } from "@/libs/utils/string";

const AppEditor = ({ pathb64 }: { pathb64: string }) => {
  const [previewKey, setPreviewKey] = useState<string>(randomKey());
  const queryClient = useQueryClient();

  const onSaved = () => {
    setPreviewKey(randomKey());
    queryClient.invalidateQueries({ queryKey: queryKeys.app.get(pathb64) });
  };

  return (
    <EditorPageWrapper
      pathb64={pathb64}
      onSaved={onSaved}
      pageContentClassName="md:flex-row flex-col"
      editorClassName="md:w-1/2 w-full h-1/2 md:h-full"
      preview={
        <div className="flex-1 overflow-hidden">
          <AppPreview key={previewKey} appPath64={pathb64 ?? ""} />
        </div>
      }
    />
  );
};
export default AppEditor;
