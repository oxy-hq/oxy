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
      {/* Hide the divider on mobile — the panel renders as a full-screen overlay there. */}
      <Separator orientation='vertical' className='hidden md:block' />
      <div className='absolute inset-0 z-20 h-full overflow-hidden bg-background md:static md:flex-1'>
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
