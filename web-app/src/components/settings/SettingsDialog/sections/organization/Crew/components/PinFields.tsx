import { Input } from "@/components/ui/shadcn/input";
import { Label } from "@/components/ui/shadcn/label";

/**
 * A PIN and its confirmation. Masked because an admin usually sets it with the
 * worker standing beside them; the confirm field is what catches a typo the
 * mask hides. `inputMode="numeric"` brings up the keypad on a tablet.
 */
export function PinFields({
  pin,
  confirm,
  onPinChange,
  onConfirmChange,
  error,
  idPrefix,
  autoFocus
}: {
  pin: string;
  confirm: string;
  onPinChange: (value: string) => void;
  onConfirmChange: (value: string) => void;
  error: string | null;
  idPrefix: string;
  autoFocus?: boolean;
}) {
  const errorId = `${idPrefix}-pin-error`;
  return (
    <div className='space-y-1.5'>
      <div className='grid grid-cols-2 gap-3'>
        <div className='space-y-1.5'>
          <Label htmlFor={`${idPrefix}-pin`}>PIN</Label>
          <Input
            id={`${idPrefix}-pin`}
            type='password'
            inputMode='numeric'
            pattern='[0-9]*'
            autoComplete='new-password'
            maxLength={8}
            placeholder='4–8 digits'
            value={pin}
            onChange={(e) => onPinChange(e.target.value)}
            autoFocus={autoFocus}
            aria-invalid={error ? true : undefined}
            aria-describedby={error ? errorId : undefined}
            className={error ? "border-destructive focus-visible:ring-destructive" : ""}
          />
        </div>
        <div className='space-y-1.5'>
          <Label htmlFor={`${idPrefix}-pin-confirm`}>Confirm PIN</Label>
          <Input
            id={`${idPrefix}-pin-confirm`}
            type='password'
            inputMode='numeric'
            pattern='[0-9]*'
            autoComplete='new-password'
            maxLength={8}
            value={confirm}
            onChange={(e) => onConfirmChange(e.target.value)}
            aria-invalid={error ? true : undefined}
            className={error ? "border-destructive focus-visible:ring-destructive" : ""}
          />
        </div>
      </div>
      {error && (
        <p id={errorId} className='text-destructive text-sm'>
          {error}
        </p>
      )}
    </div>
  );
}
