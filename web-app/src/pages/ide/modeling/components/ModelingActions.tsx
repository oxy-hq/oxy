import axios from "axios";
import { CheckCircle2, Loader2, Play, Search, Sprout, Wrench } from "lucide-react";
import type React from "react";
import { Button } from "@/components/ui/shadcn/button";
import useModelingAnalyze from "@/hooks/api/modeling/useModelingAnalyze";
import useModelingCompile from "@/hooks/api/modeling/useModelingCompile";
import useModelingSeed from "@/hooks/api/modeling/useModelingSeed";
import useModelingTest from "@/hooks/api/modeling/useModelingTest";
import type { AnalyzeOutput, CompileOutput, SeedOutput, TestOutput } from "@/types/modeling";
import type { OutputState } from "./OutputPanel";

function extractErrorMessage(error: unknown): string {
  if (axios.isAxiosError(error)) {
    const data = error.response?.data;
    if (typeof data === "string" && data.length > 0) return data;
  }
  if (error instanceof Error) return error.message;
  return "An unexpected error occurred.";
}

interface ModelingActionsProps {
  dbtProjectName: string | null;
  onOutput: (output: OutputState) => void;
  onPendingChange: (pending: boolean) => void;
  onRunStream: (selector?: string) => Promise<void>;
  onOutputKind: (kind: string) => void;
  isStreaming: boolean;
}

const ModelingActions: React.FC<ModelingActionsProps> = ({
  dbtProjectName,
  onOutput,
  onPendingChange,
  onOutputKind,
  onRunStream,
  isStreaming
}) => {
  const name = dbtProjectName ?? "";
  const compile = useModelingCompile(name);
  const test = useModelingTest(name);
  const analyze = useModelingAnalyze(name);
  const seed = useModelingSeed(name);

  const isPending =
    compile.isPending || isStreaming || test.isPending || analyze.isPending || seed.isPending;
  const disabled = !dbtProjectName || isPending;

  const handleCompile = () => {
    onOutputKind("compile");
    onPendingChange(true);
    compile.mutate(undefined, {
      onSuccess: (data: CompileOutput) => {
        onOutput({ kind: "compile", data });
        onPendingChange(false);
      },
      onError: (error) => {
        onOutput({ kind: "error", message: extractErrorMessage(error) });
        onPendingChange(false);
      }
    });
  };

  const handleSeed = () => {
    onOutputKind("seed");
    onPendingChange(true);
    seed.mutate(undefined, {
      onSuccess: (data: SeedOutput) => {
        onOutput({ kind: "seed", data });
        onPendingChange(false);
      },
      onError: (error) => {
        onOutput({ kind: "error", message: extractErrorMessage(error) });
        onPendingChange(false);
      }
    });
  };

  const handleTest = () => {
    onOutputKind("test");
    onPendingChange(true);
    test.mutate(
      {},
      {
        onSuccess: (data: TestOutput) => {
          onOutput({ kind: "test", data });
          onPendingChange(false);
        },
        onError: (error) => {
          onOutput({ kind: "error", message: extractErrorMessage(error) });
          onPendingChange(false);
        }
      }
    );
  };

  const handleAnalyze = () => {
    onOutputKind("analyze");
    onPendingChange(true);
    analyze.mutate(undefined, {
      onSuccess: (data: AnalyzeOutput) => {
        onOutput({ kind: "analyze", data });
        onPendingChange(false);
      },
      onError: (error) => {
        onOutput({ kind: "error", message: extractErrorMessage(error) });
        onPendingChange(false);
      }
    });
  };

  return (
    <div className='flex items-center gap-2 border-b px-4 py-2'>
      {dbtProjectName && (
        <span className='mr-1 font-mono text-primary text-xs'>{dbtProjectName}</span>
      )}

      <Button size='sm' variant='outline' onClick={handleCompile} disabled={disabled}>
        {compile.isPending ? (
          <Loader2 className='mr-1.5 h-3.5 w-3.5 animate-spin' />
        ) : (
          <Wrench className='mr-1.5 h-3.5 w-3.5' />
        )}
        Compile
      </Button>

      <Button size='sm' variant='outline' onClick={handleSeed} disabled={disabled}>
        {seed.isPending ? (
          <Loader2 className='mr-1.5 h-3.5 w-3.5 animate-spin' />
        ) : (
          <Sprout className='mr-1.5 h-3.5 w-3.5' />
        )}
        Seed
      </Button>

      <Button size='sm' variant='outline' onClick={() => onRunStream()} disabled={disabled}>
        {isStreaming ? (
          <Loader2 className='mr-1.5 h-3.5 w-3.5 animate-spin' />
        ) : (
          <Play className='mr-1.5 h-3.5 w-3.5' />
        )}
        Run
      </Button>

      <Button size='sm' variant='outline' onClick={handleTest} disabled={disabled}>
        {test.isPending ? (
          <Loader2 className='mr-1.5 h-3.5 w-3.5 animate-spin' />
        ) : (
          <CheckCircle2 className='mr-1.5 h-3.5 w-3.5' />
        )}
        Test
      </Button>

      <Button size='sm' variant='outline' onClick={handleAnalyze} disabled={disabled}>
        {analyze.isPending ? (
          <Loader2 className='mr-1.5 h-3.5 w-3.5 animate-spin' />
        ) : (
          <Search className='mr-1.5 h-3.5 w-3.5' />
        )}
        Analyze
      </Button>
    </div>
  );
};

export default ModelingActions;
