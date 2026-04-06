import { BrushCleaning, FlaskConical, MessageCircleDashed } from "lucide-react";
import { useState } from "react";
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/shadcn/tabs";
import AgenticAnalyticsPreview from "./Preview";
import AgenticAnalyticsTests from "./Tests";

interface PreviewSectionProps {
  pathb64: string;
  previewKey: string;
}

const PreviewSection = ({ pathb64, previewKey }: PreviewSectionProps) => {
  const [selected, setSelected] = useState("preview");
  const [resetKey, setResetKey] = useState(0);

  const handleClean = () => {
    setResetKey((k) => k + 1);
  };

  return (
    <div className='flex flex-1 flex-col overflow-hidden'>
      <div className='relative z-10 flex flex-shrink-0 justify-between bg-background p-2'>
        <Tabs value={selected} onValueChange={setSelected}>
          <TabsList>
            <TabsTrigger value='preview'>
              <MessageCircleDashed className='h-4 w-4' />
              Preview
            </TabsTrigger>
            <TabsTrigger value='test'>
              <FlaskConical />
              Test
            </TabsTrigger>
          </TabsList>
        </Tabs>
        {selected === "preview" && (
          <button
            type='button'
            onClick={handleClean}
            className='flex items-center gap-1 rounded px-2 py-1 text-muted-foreground text-sm hover:bg-accent hover:text-foreground'
            title='Clear conversation'
          >
            <BrushCleaning className='h-4 w-4' />
            Clean
          </button>
        )}
      </div>

      <div className='flex-1 overflow-hidden'>
        {selected === "preview" ? (
          <AgenticAnalyticsPreview key={`${previewKey}-${resetKey}`} pathb64={pathb64} />
        ) : (
          <AgenticAnalyticsTests key={previewKey} />
        )}
      </div>
    </div>
  );
};

export default PreviewSection;
