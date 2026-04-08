import { Avatar, AvatarFallback, AvatarImage } from "@/components/ui/shadcn/avatar";

const AVATAR_COLORS = [
  "#3550FF",
  "#7C3AED",
  "#059669",
  "#D97706",
  "#DC2626",
  "#0891B2",
  "#EA580C",
  "#4F46E5",
  "#0D9488",
  "#DB2777"
] as const;

export function avatarColor(seed: string): string {
  let hash = 0;
  for (let i = 0; i < seed.length; i++) {
    hash = (hash * 31 + seed.charCodeAt(i)) >>> 0;
  }
  return AVATAR_COLORS[hash % AVATAR_COLORS.length];
}

export function getAvatarInitials(nameOrEmail: string): string {
  return nameOrEmail
    .split(/[\s@.]+/)
    .filter(Boolean)
    .slice(0, 2)
    .map((p) => p[0].toUpperCase())
    .join("");
}

interface UserAvatarProps {
  name: string;
  email: string;
  picture?: string | null;
  className?: string;
}

export function UserAvatar({
  name,
  email,
  picture,
  className = "size-8 rounded-lg"
}: UserAvatarProps) {
  const displayName = name || email.split("@")[0];
  const initials = getAvatarInitials(displayName);
  const color = avatarColor(email);
  const fallbackClass = className.includes("rounded-md") ? "rounded-md" : "rounded-lg";

  return (
    <Avatar className={className}>
      <AvatarImage src={picture ?? undefined} alt={displayName} />
      <AvatarFallback
        className={`${fallbackClass} font-semibold text-[11px] text-white`}
        style={{ backgroundColor: color }}
      >
        {initials || "?"}
      </AvatarFallback>
    </Avatar>
  );
}
