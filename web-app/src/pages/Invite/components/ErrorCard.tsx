import { XCircle } from "lucide-react";
import { useNavigate } from "react-router-dom";
import { Button } from "@/components/ui/shadcn/button";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle
} from "@/components/ui/shadcn/card";
import ROUTES from "@/libs/utils/routes";
import type { ErrorAction } from "../types";
import { CenteredLayout } from "./CenteredLayout";

type ErrorCardProps = {
  title: string;
  description: string;
  primaryAction?: ErrorAction;
};

export function ErrorCard({ title, description, primaryAction }: ErrorCardProps) {
  const navigate = useNavigate();
  return (
    <CenteredLayout>
      <Card className='w-full max-w-md'>
        <CardHeader className='text-center'>
          <div className='mb-4 flex justify-center'>
            <XCircle className='h-12 w-12 text-destructive' />
          </div>
          <CardTitle className='text-2xl'>{title}</CardTitle>
          <CardDescription>{description}</CardDescription>
        </CardHeader>
        <CardContent className='flex flex-col gap-2'>
          {primaryAction && (
            <Button onClick={primaryAction.onClick} className='w-full'>
              {primaryAction.label}
            </Button>
          )}
          <Button
            variant={primaryAction ? "outline" : "default"}
            onClick={() => navigate(ROUTES.ROOT)}
            className='w-full'
          >
            Back to home
          </Button>
        </CardContent>
      </Card>
    </CenteredLayout>
  );
}
