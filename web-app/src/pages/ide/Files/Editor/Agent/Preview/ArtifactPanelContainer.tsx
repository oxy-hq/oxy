import dayjs from "dayjs";
import relativeTime from "dayjs/plugin/relativeTime";

import ArtifactPanel from "@/components/ArtifactPanel";
import { Dialog, DialogContent } from "@/components/ui/shadcn/dialog";
import type { Message } from "@/types/chat";

dayjs.extend(relativeTime);

interface Props {
  selectedIds: string[];
  onSelect: React.Dispatch<React.SetStateAction<string[]>>;
  messages: Message[];
}

const ArtifactPanelContainer = ({ selectedIds, onSelect, messages }: Props) => {
  const artifactStreamingData = messages.reduce((acc, msg) => {
    if (msg.artifacts) {
      acc = { ...acc, ...msg.artifacts };
    }
    return acc;
  }, {});

  if (selectedIds.length <= 0) {
    return null;
  }

  return (
    <Dialog open={selectedIds.length > 0} onOpenChange={() => onSelect([])}>
      <DialogContent
        onOpenAutoFocus={(e) => e.preventDefault()}
        showCloseButton={false}
        className='h-[80vh] w-full max-w-4xl! p-0'
      >
        <div className='h-[80vh]'>
          <ArtifactPanel
            selectedArtifactIds={selectedIds}
            artifactStreamingData={artifactStreamingData}
            onClose={() => onSelect([])}
            setSelectedArtifactIds={onSelect}
          />
        </div>
      </DialogContent>
    </Dialog>
  );
};

export default ArtifactPanelContainer;
