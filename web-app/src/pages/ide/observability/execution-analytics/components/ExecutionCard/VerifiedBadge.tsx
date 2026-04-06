import { ShieldCheck, Sparkles } from "lucide-react";

interface VerifiedBadgeProps {
  isVerified: boolean;
}

export default function VerifiedBadge({ isVerified }: VerifiedBadgeProps) {
  if (isVerified) {
    return (
      <span className='inline-flex items-center gap-1.5 rounded-full border border-success/20 bg-success/10 px-2.5 py-1 font-medium text-success text-xs'>
        <ShieldCheck className='h-3.5 w-3.5' />
        Verified
      </span>
    );
  }

  return (
    <span className='inline-flex items-center gap-1.5 rounded-full border border-vis-orange/20 bg-vis-orange/10 px-2.5 py-1 font-medium text-vis-orange text-xs'>
      <Sparkles className='h-3.5 w-3.5' />
      Generated
    </span>
  );
}
