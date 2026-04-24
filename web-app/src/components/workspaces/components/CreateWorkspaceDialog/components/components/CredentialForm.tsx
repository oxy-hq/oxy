import { ArrowRight, FileIcon, Loader2, UploadCloud } from "lucide-react";
import { type ChangeEvent, type DragEvent, useCallback, useRef, useState } from "react";
import { SecretInput } from "@/components/ui/SecretInput";
import { Button } from "@/components/ui/shadcn/button";
import { Input } from "@/components/ui/shadcn/input";
import { cn } from "@/libs/shadcn/utils";
import type { CredentialField } from "../types";

/**
 * Per-field state for `file_upload` fields. Tracks the files the user picked,
 * whether an upload is in flight, which paths the backend confirmed, and any
 * error from the last upload attempt. One instance is keyed per field, so a
 * form can hypothetically carry multiple upload fields — today only DuckDB
 * uses this.
 */
interface FileUploadState {
  selected: File[];
  uploading: boolean;
  uploadedPaths: string[];
  subdir?: string;
  error?: string;
}

/** Result returned by the parent's upload callback. */
export interface FileUploadResult {
  subdir: string;
  files: string[];
}

interface CredentialFormProps {
  fields: CredentialField[];
  buttonLabel?: string;
  onSubmit: (values: Record<string, string>) => void;
  disabled?: boolean;
  initialValues?: Record<string, string>;
  /**
   * Inline error surfaced below the form. Used by the GitHub onboarding flow
   * to show a failed connection-test message so the user can fix typos and
   * retry without losing what they've already typed.
   */
  errorMessage?: string;
  /**
   * Called when the user picks files for a `file_upload` field. Should upload
   * the files and resolve with the server-assigned subdir + file paths, or
   * reject (the form surfaces the error inline).
   */
  onFileUpload?: (files: File[]) => Promise<FileUploadResult | null>;
}

export default function CredentialForm({
  fields,
  buttonLabel = "Continue",
  onSubmit,
  disabled,
  initialValues,
  errorMessage,
  onFileUpload
}: CredentialFormProps) {
  const [values, setValues] = useState<Record<string, string>>(() => {
    const initial: Record<string, string> = {};
    for (const field of fields) {
      // Prefer initialValues (from previous attempt), then defaultValue
      if (initialValues?.[field.key]) {
        initial[field.key] = initialValues[field.key];
      } else if (field.defaultValue) {
        initial[field.key] = field.defaultValue;
      }
    }
    return initial;
  });

  const [uploads, setUploads] = useState<Record<string, FileUploadState>>({});

  const handleChange = useCallback((key: string, value: string) => {
    setValues((prev) => ({ ...prev, [key]: value }));
  }, []);

  const isFieldFilled = useCallback(
    (field: CredentialField) => {
      if (!field.required) return true;
      if (field.type === "file_upload") {
        const state = uploads[field.key];
        return !!state && state.uploadedPaths.length > 0 && !state.uploading;
      }
      const v = values[field.key];
      return typeof v === "string" && v.trim() !== "";
    },
    [uploads, values]
  );

  const allRequiredFilled = fields.every(isFieldFilled);
  const anyUploading = Object.values(uploads).some((s) => s.uploading);

  const handleSubmit = useCallback(() => {
    if (!allRequiredFilled || anyUploading) return;
    onSubmit(values);
  }, [allRequiredFilled, anyUploading, onSubmit, values]);

  const runUpload = useCallback(
    async (fieldKey: string, files: File[]) => {
      if (files.length === 0 || !onFileUpload) return;
      setUploads((prev) => ({
        ...prev,
        [fieldKey]: {
          selected: [...(prev[fieldKey]?.selected ?? []), ...files],
          uploading: true,
          uploadedPaths: prev[fieldKey]?.uploadedPaths ?? [],
          subdir: prev[fieldKey]?.subdir,
          error: undefined
        }
      }));
      try {
        const result = await onFileUpload(files);
        setUploads((prev) => {
          const current = prev[fieldKey];
          if (!current) return prev;
          if (!result) {
            return {
              ...prev,
              [fieldKey]: { ...current, uploading: false, error: "Upload failed." }
            };
          }
          return {
            ...prev,
            [fieldKey]: {
              ...current,
              uploading: false,
              uploadedPaths: [...current.uploadedPaths, ...result.files],
              subdir: result.subdir
            }
          };
        });
        if (result) {
          // Seed the form value with the server-chosen subdir so the parent
          // gets a meaningful value on submit (e.g. ".db" for DuckDB).
          handleChange(fieldKey, result.subdir);
        }
      } catch (err) {
        setUploads((prev) => {
          const current = prev[fieldKey];
          if (!current) return prev;
          return {
            ...prev,
            [fieldKey]: {
              ...current,
              uploading: false,
              error: err instanceof Error ? err.message : "Upload failed."
            }
          };
        });
      }
    },
    [onFileUpload, handleChange]
  );

  return (
    <div className='flex flex-col gap-3'>
      <div className='grid grid-cols-2 gap-3'>
        {fields.map((field) => {
          const inputId = `credential-${field.key}`;
          if (field.type === "file_upload") {
            return (
              <div key={field.key} className='col-span-2'>
                <label htmlFor={inputId} className='mb-1 block text-muted-foreground text-xs'>
                  {field.label}
                  {field.required && <span className='text-destructive'> *</span>}
                </label>
                <FileUploadField
                  inputId={inputId}
                  placeholder={field.placeholder}
                  helperText={field.helperText}
                  accept={field.accept ?? ""}
                  multiple={field.multiple ?? false}
                  disabled={disabled}
                  state={uploads[field.key]}
                  onPick={(files) => runUpload(field.key, files)}
                />
              </div>
            );
          }
          const isPassword = field.type === "password";
          const InputComponent = isPassword ? SecretInput : Input;
          return (
            <div key={field.key} className={isPassword ? "col-span-2" : undefined}>
              <label htmlFor={inputId} className='mb-1 block text-muted-foreground text-xs'>
                {field.label}
                {field.required && <span className='text-destructive'> *</span>}
              </label>
              <InputComponent
                id={inputId}
                value={values[field.key] ?? ""}
                onChange={(e) => handleChange(field.key, e.target.value)}
                placeholder={field.placeholder}
                disabled={disabled}
                className='font-mono text-sm'
                onKeyDown={(e) => {
                  if (e.key === "Enter" && allRequiredFilled && !disabled) handleSubmit();
                }}
              />
            </div>
          );
        })}
      </div>
      {errorMessage && (
        <p className='rounded-md border border-destructive/40 bg-destructive/10 px-3 py-2 text-destructive text-xs'>
          {errorMessage}
        </p>
      )}
      <div className='flex justify-end'>
        <Button
          onClick={handleSubmit}
          disabled={disabled || !allRequiredFilled || anyUploading}
          size='sm'
        >
          {buttonLabel}
          <ArrowRight className='ml-1 h-3 w-3' />
        </Button>
      </div>
    </div>
  );
}

interface FileUploadFieldProps {
  inputId: string;
  placeholder: string;
  helperText?: string;
  accept: string;
  multiple: boolean;
  disabled?: boolean;
  state?: FileUploadState;
  onPick: (files: File[]) => void;
}

function FileUploadField({
  inputId,
  placeholder,
  helperText,
  accept,
  multiple,
  disabled,
  state,
  onPick
}: FileUploadFieldProps) {
  const inputRef = useRef<HTMLInputElement | null>(null);
  const [isDragging, setIsDragging] = useState(false);

  const handleFiles = useCallback(
    (list: FileList | null) => {
      if (!list || list.length === 0) return;
      onPick(Array.from(list));
    },
    [onPick]
  );

  const handleDrop = useCallback(
    (e: DragEvent<HTMLButtonElement>) => {
      e.preventDefault();
      e.stopPropagation();
      setIsDragging(false);
      if (disabled) return;
      handleFiles(e.dataTransfer?.files ?? null);
    },
    [disabled, handleFiles]
  );

  const handleDragOver = useCallback(
    (e: DragEvent<HTMLButtonElement>) => {
      e.preventDefault();
      e.stopPropagation();
      if (disabled) return;
      setIsDragging(true);
    },
    [disabled]
  );

  const handleDragLeave = useCallback((e: DragEvent<HTMLButtonElement>) => {
    e.preventDefault();
    e.stopPropagation();
    setIsDragging(false);
  }, []);

  const handleChange = useCallback(
    (e: ChangeEvent<HTMLInputElement>) => {
      handleFiles(e.target.files);
      // Reset so picking the same file again still fires change.
      e.target.value = "";
    },
    [handleFiles]
  );

  const hasUploads = (state?.uploadedPaths.length ?? 0) > 0;
  const isUploading = state?.uploading ?? false;

  return (
    <div className='flex flex-col gap-2'>
      <button
        type='button'
        onClick={() => inputRef.current?.click()}
        onDragOver={handleDragOver}
        onDragLeave={handleDragLeave}
        onDrop={handleDrop}
        disabled={disabled || isUploading}
        className={cn(
          "flex min-h-24 w-full flex-col items-center justify-center gap-2 rounded-md border border-border border-dashed bg-muted/30 px-4 py-6 text-muted-foreground text-sm transition-colors hover:border-primary hover:text-foreground",
          isDragging && "border-primary bg-muted text-foreground",
          disabled && "cursor-not-allowed opacity-60"
        )}
      >
        {isUploading ? (
          <Loader2 className='h-5 w-5 animate-spin' />
        ) : (
          <UploadCloud className='h-5 w-5' />
        )}
        <span className='text-center'>{isUploading ? "Uploading…" : placeholder}</span>
        {accept && (
          <span className='text-muted-foreground text-xs'>
            Accepts{" "}
            {accept
              .split(",")
              .map((a) => a.trim())
              .join(" and ")}
          </span>
        )}
      </button>
      <input
        ref={inputRef}
        id={inputId}
        type='file'
        accept={accept}
        multiple={multiple}
        className='hidden'
        onChange={handleChange}
        disabled={disabled || isUploading}
      />
      {helperText && <p className='text-muted-foreground text-xs'>{helperText}</p>}
      {state?.error && <p className='text-destructive text-xs'>{state.error}</p>}
      {hasUploads && state && (
        <ul className='flex flex-col gap-1'>
          {state.uploadedPaths.map((path) => (
            <li
              key={path}
              className='flex items-center gap-2 rounded border border-border bg-muted/40 px-2 py-1 text-xs'
            >
              <FileIcon className='h-3 w-3 text-muted-foreground' />
              <span className='flex-1 truncate font-mono'>{path}</span>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
