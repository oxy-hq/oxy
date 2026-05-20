import { Eye, EyeOff } from "lucide-react";
import { toast } from "sonner";
import { Button } from "@/components/ui/shadcn/button";
import { Spinner } from "@/components/ui/shadcn/spinner";
import useApps from "@/hooks/api/apps/useApps";
import usePublishApp from "@/hooks/api/apps/usePublishApp";
import { decodeBase64 } from "@/libs/encoding";

type Props = {
  pathb64: string;
};

export const PublishToggle = ({ pathb64 }: Props) => {
  const filePath = decodeBase64(pathb64);
  const { data: apps } = useApps();
  const publish = usePublishApp();

  const app = apps?.find((a) => a.path === filePath);

  // Hide entirely for users who can't publish (Viewer) or while we don't know yet.
  if (!app || app.can_publish === false) return null;

  const isPublished = !!app.published;
  const isPending = publish.isPending;

  const handleClick = () => {
    publish.mutate(
      { pathb64, publish: !isPublished },
      {
        onSuccess: () => {
          toast.success(isPublished ? "App unpublished" : "App published");
        },
        onError: (err) => {
          const message = err instanceof Error ? err.message : "Failed to update publish state";
          toast.error(message);
        }
      }
    );
  };

  return (
    <Button
      size='sm'
      variant={isPublished ? "outline" : "default"}
      onClick={handleClick}
      disabled={isPending}
    >
      {isPending ? (
        <Spinner />
      ) : isPublished ? (
        <>
          <EyeOff />
          Unpublish
        </>
      ) : (
        <>
          <Eye />
          Publish
        </>
      )}
    </Button>
  );
};
