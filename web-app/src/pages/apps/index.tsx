import { ExternalLink } from "lucide-react";
import PageHeader from "@/components/PageHeader";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/shadcn/card";
import { Spinner } from "@/components/ui/shadcn/spinner";
import { useMyApps } from "@/hooks/api/customerApps/useCustomerApps";

export default function AppsPage() {
  const { data: apps, isLoading } = useMyApps();

  return (
    <div className='flex h-full w-full flex-col'>
      <PageHeader />
      <div className='flex flex-1 flex-col gap-4 overflow-auto p-6'>
        <div>
          <h1 className='font-semibold text-2xl'>Apps</h1>
          <p className='text-muted-foreground text-sm'>
            Customer-facing dashboards your team has access to.
          </p>
        </div>

        {isLoading && (
          <div className='flex items-center justify-center py-12'>
            <Spinner />
          </div>
        )}

        {!isLoading && (!apps || apps.length === 0) && (
          <div className='py-12 text-center'>
            <p className='text-muted-foreground'>You don&apos;t have access to any apps yet.</p>
          </div>
        )}

        {!isLoading && apps && apps.length > 0 && (
          <div className='grid grid-cols-1 gap-3 sm:grid-cols-2 lg:grid-cols-3'>
            {apps.map((app) => (
              <a key={app.id} href={app.url} target='_blank' rel='noopener noreferrer'>
                <Card className='transition-colors hover:border-primary'>
                  <CardHeader>
                    <CardTitle className='flex items-center gap-2'>
                      {app.name}
                      <ExternalLink className='h-4 w-4' />
                    </CardTitle>
                  </CardHeader>
                  <CardContent>
                    <p className='text-muted-foreground text-sm'>{app.url}</p>
                  </CardContent>
                </Card>
              </a>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}
