import { AlertCircle, Eye, EyeOff } from "lucide-react";
import { useState } from "react";
import { Button } from "@/components/ui/shadcn/button";
import { Input } from "@/components/ui/shadcn/input";

interface OpenAIKeyStepProps {
  initialData?: string;
  isCreating?: boolean;
  onNext: (data: string) => void | Promise<void>;
  onBack: () => void;
}

export default function OpenAIKeyStep({
  initialData,
  isCreating = false,
  onNext,
  onBack
}: OpenAIKeyStepProps) {
  const [apiKey, setApiKey] = useState(initialData || "");
  const [showKey, setShowKey] = useState(false);
  const [error, setError] = useState("");

  const handleNext = async () => {
    if (!apiKey.trim()) {
      setError("OpenAI API key is required");
      return;
    }

    if (!apiKey.startsWith("sk-")) {
      setError("Invalid OpenAI API key format");
      return;
    }

    setError("");
    await onNext(apiKey);
  };

  return (
    <div className='space-y-6'>
      <div>
        <h2 className='font-bold text-2xl'>OpenAI API Key</h2>
        <p className='mt-2 text-muted-foreground'>
          Enter your OpenAI API key to enable AI features in the demo project.
        </p>
      </div>

      <div className='space-y-4'>
        {error && (
          <div className='flex items-start gap-2 rounded-md border border-destructive/20 bg-destructive/10 p-3'>
            <AlertCircle className='mt-0.5 h-4 w-4 flex-shrink-0 text-destructive' />
            <p className='text-destructive text-sm'>{error}</p>
          </div>
        )}

        <div>
          <label htmlFor='api-key' className='font-medium text-sm'>
            API Key
          </label>
          <div className='relative mt-2'>
            <Input
              id='api-key'
              type={showKey ? "text" : "password"}
              placeholder='sk-...'
              value={apiKey}
              onChange={(e: React.ChangeEvent<HTMLInputElement>) => {
                setApiKey(e.target.value);
                setError("");
              }}
              className='pr-10'
            />
            <button
              type='button'
              onClick={() => setShowKey(!showKey)}
              className='absolute top-1/2 right-3 -translate-y-1/2 text-muted-foreground transition-colors hover:text-foreground'
            >
              {showKey ? <EyeOff className='h-4 w-4' /> : <Eye className='h-4 w-4' />}
            </button>
          </div>
          <p className='mt-1 text-muted-foreground text-xs'>
            Get your API key from{" "}
            <a
              href='https://platform.openai.com/api-keys'
              target='_blank'
              rel='noopener noreferrer'
              className='text-primary hover:underline'
            >
              platform.openai.com/api-keys
            </a>
          </p>
        </div>
      </div>

      <div className='flex gap-3'>
        <Button variant='outline' onClick={onBack} disabled={isCreating}>
          Back
        </Button>
        <Button onClick={handleNext} disabled={isCreating || !apiKey.trim()}>
          {isCreating ? "Creating..." : "Next"}
        </Button>
      </div>
    </div>
  );
}
