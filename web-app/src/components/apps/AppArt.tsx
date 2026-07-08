import { useState } from "react";
import { cn } from "@/libs/shadcn/utils";

/**
 * The app's wide preview image (its manifest `art`, resolved server-side to
 * `art_url`) with an initial-letter tile fallback — the ONE way any surface
 * renders an app screenshot. Author-supplied only; there is no screenshot
 * capture. See the `oxy-app-visual-identity` skill.
 */
export function AppArt({
  artUrl,
  name,
  className
}: {
  artUrl?: string | null;
  name: string;
  className?: string;
}) {
  const [failed, setFailed] = useState(false);

  if (!artUrl || failed) {
    return (
      <div
        className={cn(
          "flex h-40 w-full items-center justify-center rounded-md border bg-primary/10",
          className
        )}
      >
        <span className='font-semibold text-4xl text-primary'>
          {name.slice(0, 1).toUpperCase()}
        </span>
      </div>
    );
  }
  return (
    <img
      src={artUrl}
      alt=''
      loading='lazy'
      onError={() => setFailed(true)}
      className={cn("h-40 w-full rounded-md border object-cover", className)}
    />
  );
}
