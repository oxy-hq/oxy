import { useState } from "react";
import { Button } from "@/components/ui/shadcn/button";
import { Input } from "@/components/ui/shadcn/input";
import { AlertCircle, Eye, EyeOff } from "lucide-react";

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
  onBack,
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
    <div className="space-y-6">
      <div>
        <h2 className="text-2xl font-bold">OpenAI API Key</h2>
        <p className="text-muted-foreground mt-2">
          Enter your OpenAI API key to enable AI features in the demo project.
        </p>
      </div>

      <div className="space-y-4">
        {error && (
          <div className="flex items-start gap-2 p-3 bg-destructive/10 border border-destructive/20 rounded-md">
            <AlertCircle className="h-4 w-4 text-destructive mt-0.5 flex-shrink-0" />
            <p className="text-sm text-destructive">{error}</p>
          </div>
        )}

        <div>
          <label htmlFor="api-key" className="text-sm font-medium">
            API Key
          </label>
          <div className="relative mt-2">
            <Input
              id="api-key"
              type={showKey ? "text" : "password"}
              placeholder="sk-..."
              value={apiKey}
              onChange={(e: React.ChangeEvent<HTMLInputElement>) => {
                setApiKey(e.target.value);
                setError("");
              }}
              className="pr-10"
            />
            <button
              type="button"
              onClick={() => setShowKey(!showKey)}
              className="absolute right-3 top-1/2 -translate-y-1/2 text-muted-foreground hover:text-foreground transition-colors"
            >
              {showKey ? (
                <EyeOff className="h-4 w-4" />
              ) : (
                <Eye className="h-4 w-4" />
              )}
            </button>
          </div>
          <p className="text-xs text-muted-foreground mt-1">
            Get your API key from{" "}
            <a
              href="https://platform.openai.com/api-keys"
              target="_blank"
              rel="noopener noreferrer"
              className="text-primary hover:underline"
            >
              platform.openai.com/api-keys
            </a>
          </p>
        </div>
      </div>

      <div className="flex gap-3">
        <Button variant="outline" onClick={onBack} disabled={isCreating}>
          Back
        </Button>
        <Button onClick={handleNext} disabled={isCreating || !apiKey.trim()}>
          {isCreating ? "Creating..." : "Next"}
        </Button>
      </div>
    </div>
  );
}
