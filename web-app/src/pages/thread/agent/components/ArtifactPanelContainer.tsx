import dayjs from "dayjs";
import relativeTime from "dayjs/plugin/relativeTime";

import ArtifactPanel from "@/components/ArtifactPanel";
import { Separator } from "@/components/ui/shadcn/separator";
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
    <>
      <Separator orientation='vertical' />
      <div className='h-full flex-1 overflow-hidden'>
        <ArtifactPanel
          selectedArtifactIds={selectedIds}
          artifactStreamingData={artifactStreamingData}
          onClose={() => onSelect([])}
          setSelectedArtifactIds={onSelect}
        />
      </div>
    </>
  );
};

export default ArtifactPanelContainer;
