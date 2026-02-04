import { DisplayBlock } from "@/components/AppPreview/Displays";
import type { BlockBase, VizContent } from "@/services/types";
import type { Display, TableDisplay } from "@/types/app";

const Visualization = ({ block }: { block: BlockBase & VizContent }) => {
  return (
    <div className='flex h-full flex-col p-4'>
      <DisplayBlock
        display={block.config as Display}
        data={{
          [(block.config as TableDisplay).data]: {
            file_path: (block.config as TableDisplay).data
          }
        }}
      />
    </div>
  );
};

export default Visualization;
