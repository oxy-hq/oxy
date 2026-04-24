import { ArrowRight, Eye, EyeOff } from "lucide-react";
import { useCallback, useState } from "react";
import { Button } from "@/components/ui/shadcn/button";
import { Input } from "@/components/ui/shadcn/input";

interface SecureInputProps {
  label: string;
  placeholder: string;
  buttonLabel?: string;
  onSubmit: (value: string) => void;
  disabled?: boolean;
}

export default function SecureInput({
  label,
  placeholder,
  buttonLabel = "Continue",
  onSubmit,
  disabled
}: SecureInputProps) {
  const [value, setValue] = useState("");
  const [visible, setVisible] = useState(false);

  const handleSubmit = useCallback(() => {
    const trimmed = value.trim();
    if (trimmed) onSubmit(trimmed);
  }, [value, onSubmit]);

  return (
    <div className='flex flex-col gap-2'>
      <label htmlFor='secure-input' className='text-muted-foreground text-xs'>
        {label}
      </label>
      <div className='flex gap-2'>
        <div className='relative flex-1'>
          <Input
            id='secure-input'
            type={visible ? "text" : "password"}
            value={value}
            onChange={(e) => setValue(e.target.value)}
            placeholder={placeholder}
            disabled={disabled}
            className='pr-10 font-mono text-sm'
            onKeyDown={(e) => {
              if (e.key === "Enter") handleSubmit();
            }}
          />
          <button
            type='button'
            onClick={() => setVisible(!visible)}
            className='absolute top-1/2 right-3 -translate-y-1/2 text-muted-foreground hover:text-foreground'
          >
            {visible ? <EyeOff className='h-4 w-4' /> : <Eye className='h-4 w-4' />}
          </button>
        </div>
        <Button onClick={handleSubmit} disabled={disabled || !value.trim()} size='sm'>
          {buttonLabel}
          <ArrowRight className='ml-1 h-3 w-3' />
        </Button>
      </div>
    </div>
  );
}
