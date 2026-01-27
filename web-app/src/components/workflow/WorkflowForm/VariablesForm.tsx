import React, { useMemo, useState } from "react";
import { useFormContext } from "react-hook-form";
import { Label } from "@/components/ui/shadcn/label";
import { Button } from "@/components/ui/shadcn/button";
import { Editor } from "@monaco-editor/react";
import { Loader2 } from "lucide-react";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from "@/components/ui/shadcn/dialog";
import { Info } from "lucide-react";
import { WorkflowFormData } from "./index";

export const VariablesForm: React.FC = () => {
  const { setValue, watch } = useFormContext<WorkflowFormData>();
  const [isJsonValid, setIsJsonValid] = useState(true);
  const [jsonError, setJsonError] = useState<string>("");

  const currentVariables = watch("variables") || "";

  const defaultSchema = `{
  "user_name": {
    "type": "string",
    "description": "Name of the user"
  },
  "age": {
    "type": "integer",
    "minimum": 0,
    "maximum": 150
  },
  "is_active": {
    "type": "boolean",
    "default": true
  }
}`;

  const variableStr = useMemo(() => {
    if (currentVariables && typeof currentVariables === "string") {
      return currentVariables;
    }

    if (
      currentVariables &&
      (typeof currentVariables === "object" || Array.isArray(currentVariables))
    ) {
      return JSON.stringify(currentVariables, null, 2);
    }
  }, [currentVariables]);

  const validateAndSetVariables = (jsonString: string) => {
    if (jsonString.trim() === "") {
      setIsJsonValid(true);
      setJsonError("");
      setValue("variables", undefined);
      return;
    }

    try {
      const value = JSON.parse(jsonString);
      setValue("variables", value);
      setIsJsonValid(true);
      setJsonError("");
    } catch (error) {
      setValue("variables", jsonString);
      setIsJsonValid(false);
      setJsonError(error instanceof Error ? error.message : "Invalid JSON");
    }
  };

  return (
    <div className="space-y-4">
      <div className="space-y-2">
        <Label className="flex justify-between" htmlFor="variables-schema">
          <p>Variables Schema (JSON)</p>
          <Dialog>
            <DialogTrigger asChild>
              <Button variant="ghost" size="sm" className="h-6 w-6 p-0">
                <Info className="h-4 w-4" />
              </Button>
            </DialogTrigger>
            <DialogContent className="max-w-2xl">
              <DialogHeader>
                <DialogTitle>JSON Schema Documentation</DialogTitle>
              </DialogHeader>
              <div className="space-y-4">
                <div className="grid grid-cols-1 md:grid-cols-2 gap-4 text-sm">
                  <div className="space-y-2">
                    <h5 className="font-medium">Supported Types:</h5>
                    <ul className="text-muted-foreground space-y-1">
                      <li>
                        • <code>string</code> - Text values
                      </li>
                      <li>
                        • <code>number</code> - Decimal numbers
                      </li>
                      <li>
                        • <code>integer</code> - Whole numbers
                      </li>
                      <li>
                        • <code>boolean</code> - true/false values
                      </li>
                      <li>
                        • <code>array</code> - List of items
                      </li>
                      <li>
                        • <code>object</code> - Nested objects
                      </li>
                    </ul>
                  </div>
                  <div className="space-y-2">
                    <h5 className="font-medium">Common Properties:</h5>
                    <ul className="text-muted-foreground space-y-1">
                      <li>
                        • <code>description</code> - Help text
                      </li>
                      <li>
                        • <code>default</code> - Default value
                      </li>
                      <li>
                        • <code>enum</code> - Allowed values
                      </li>
                      <li>
                        • <code>minimum/maximum</code> - Number limits
                      </li>
                      <li>
                        • <code>minLength/maxLength</code> - String length
                      </li>
                      <li>
                        • <code>required</code> - Required fields array
                      </li>
                    </ul>
                  </div>
                </div>

                <div className="p-4 bg-blue-50 rounded-lg">
                  <h5 className="font-medium text-blue-800 mb-2">
                    Example Schema:
                  </h5>
                  <pre className="text-xs text-blue-700 overflow-x-auto">
                    {defaultSchema}
                  </pre>
                </div>

                <div className="p-4 bg-amber-50 rounded-lg">
                  <p className="text-sm text-amber-800">
                    <strong>Note:</strong> Variables define the schema for
                    inputs that can be provided when running the automation. The
                    JSON should be a valid JSON Schema object where each key is
                    a variable name and the value is its schema definition.
                  </p>
                </div>
              </div>
            </DialogContent>
          </Dialog>
        </Label>
        <div
          className={`border rounded-md overflow-hidden ${!isJsonValid ? "border-red-500" : "border-input"}`}
        >
          <Editor
            height="300px"
            width="100%"
            theme="vs-dark"
            language="json"
            value={variableStr || ""}
            loading={
              <Loader2 className="w-4 h-4 animate-[spin_0.2s_linear_infinite] text-[white]" />
            }
            options={{
              minimap: { enabled: false },
              scrollBeyondLastLine: false,
              formatOnPaste: true,
              formatOnType: true,
              automaticLayout: true,
              wordWrap: "on",
              lineNumbers: "on",
              glyphMargin: false,
              folding: true,
              lineDecorationsWidth: 0,
              lineNumbersMinChars: 3,
            }}
            onChange={(value) => validateAndSetVariables(value || "")}
          />
        </div>
        {!isJsonValid && (
          <p className="text-sm text-red-500">Invalid JSON: {jsonError}</p>
        )}
      </div>
    </div>
  );
};
