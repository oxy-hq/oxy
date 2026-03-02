import type { AxiosError } from "axios";
import { Loader2, XCircle } from "lucide-react";
import type React from "react";
import { useEffect } from "react";
import { useNavigate, useSearchParams } from "react-router-dom";
import { Button } from "@/components/ui/shadcn/button";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle
} from "@/components/ui/shadcn/card";
import { useVerifyMagicLink } from "@/hooks/auth/useMagicLink";
import ROUTES from "@/libs/utils/routes";

const MagicLinkCallback: React.FC = () => {
  const [searchParams] = useSearchParams();
  const navigate = useNavigate();
  const { mutate: verifyMagicLink, status, error } = useVerifyMagicLink();

  useEffect(() => {
    const token = searchParams.get("token");
    if (!token) return;

    if (status === "idle") {
      verifyMagicLink(
        { token },
        {
          onSuccess: () => {
            navigate(ROUTES.ROOT, { replace: true });
          }
        }
      );
    }
  }, [searchParams, verifyMagicLink, status, navigate]);

  const token = searchParams.get("token");

  if (!token) {
    return (
      <div className='flex min-h-screen w-full items-center justify-center bg-background p-4'>
        <Card className='w-full max-w-md'>
          <CardHeader className='text-center'>
            <div className='mb-4 flex justify-center'>
              <XCircle className='h-12 w-12 text-red-500' />
            </div>
            <CardTitle className='text-2xl'>Invalid link</CardTitle>
            <CardDescription>This sign-in link is missing or malformed.</CardDescription>
          </CardHeader>
          <CardContent>
            <Button onClick={() => navigate(ROUTES.AUTH.LOGIN)} className='w-full'>
              Back to login
            </Button>
          </CardContent>
        </Card>
      </div>
    );
  }

  if (status === "error") {
    const isExpired = (error as AxiosError)?.response?.status === 401;
    return (
      <div className='flex min-h-screen w-full items-center justify-center bg-background p-4'>
        <Card className='w-full max-w-md'>
          <CardHeader className='text-center'>
            <div className='mb-4 flex justify-center'>
              <XCircle className='h-12 w-12 text-red-500' />
            </div>
            <CardTitle className='text-2xl'>Link expired</CardTitle>
            <CardDescription>
              {isExpired
                ? "This sign-in link has expired or already been used."
                : "This sign-in link is invalid or has expired."}
            </CardDescription>
          </CardHeader>
          <CardContent>
            <Button onClick={() => navigate(ROUTES.AUTH.LOGIN)} className='w-full'>
              Request a new sign-in link
            </Button>
          </CardContent>
        </Card>
      </div>
    );
  }

  return (
    <div className='flex min-h-screen w-full items-center justify-center bg-background p-4'>
      <div className='flex flex-col items-center gap-4 text-center'>
        <Loader2 className='h-10 w-10 animate-spin text-primary' />
        <p className='text-muted-foreground text-sm'>Signing you in…</p>
      </div>
    </div>
  );
};

export default MagicLinkCallback;
