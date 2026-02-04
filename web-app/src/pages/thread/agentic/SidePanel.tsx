import { X } from "lucide-react";
import Reasoning from "@/components/Reasoning";
import {
  Breadcrumb,
  BreadcrumbItem,
  BreadcrumbLink,
  BreadcrumbList,
  BreadcrumbSeparator
} from "@/components/ui/shadcn/breadcrumb";
import { Button } from "@/components/ui/shadcn/button";
import { Separator } from "@/components/ui/shadcn/separator";
import type { Block } from "@/services/types";
import { useSelectedMessageReasoning } from "@/stores/agentic";
import DataApp from "./AgenticArtifacts/DataApp";
import ExecuteSQL from "./AgenticArtifacts/ExecuteSQL";
import Visualization from "./AgenticArtifacts/Visualization";

const SidePanel = () => {
  const { selectedGroupId, reasoningSteps, selectedBlock, setSelectedBlockId, setSelectedGroupId } =
    useSelectedMessageReasoning();

  const renderArtifact = () => {
    if (!selectedBlock) return null;

    switch (selectedBlock.type) {
      case "data_app":
        return <DataApp pathb64={btoa(selectedBlock.file_path)} />;
      case "sql":
        return <ExecuteSQL block={selectedBlock} />;
      case "viz":
        return <Visualization block={selectedBlock} />;
      default:
        return (
          <div className='flex h-full items-center justify-center p-4'>
            <p>Unsupported artifact type: {selectedBlock.type}</p>
          </div>
        );
    }
  };

  const renderContent = () => {
    if (selectedBlock) {
      return <div className='relative h-full w-full overflow-hidden'>{renderArtifact()}</div>;
    }

    return selectedGroupId ? (
      <Reasoning steps={reasoningSteps} onFullscreen={setSelectedBlockId} />
    ) : (
      <div className='relative h-full w-full overflow-hidden'>
        Please select a reasoning step to view details.
      </div>
    );
  };

  return (
    <>
      <Separator orientation='vertical' />
      <div className='flex h-full w-full flex-1 flex-col overflow-hidden'>
        <Header
          block={selectedBlock || undefined}
          onBack={() => {
            setSelectedBlockId(null);
          }}
          onClose={() => {
            setSelectedGroupId(null);
          }}
        />
        {renderContent()}
      </div>
    </>
  );
};

const Header = ({
  block,
  onBack,
  onClose
}: {
  block?: Block;
  onBack: () => void;
  onClose: () => void;
}) => {
  return (
    <div className='flex w-full px-4 py-2 align-center'>
      <Breadcrumb className='flex flex-1 items-center align-center'>
        <BreadcrumbList>
          <BreadcrumbItem>
            <BreadcrumbLink className='cursor-pointer' onClick={onBack}>
              Reasoning
            </BreadcrumbLink>
          </BreadcrumbItem>
          {!!block && (
            <>
              <BreadcrumbSeparator />
              <BreadcrumbItem>{`${block.type} artifact`}</BreadcrumbItem>
            </>
          )}
        </BreadcrumbList>
      </Breadcrumb>
      <div className='flex gap-2'>
        <Button variant='outline' size='icon' onClick={onClose}>
          <X />
        </Button>
      </div>
    </div>
  );
};

export default SidePanel;
