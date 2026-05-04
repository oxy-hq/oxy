import { useNavigate, useParams } from "react-router-dom";
import { Button } from "@/components/ui/shadcn/button";
import ROUTES from "@/libs/utils/routes";

const SUPPORT_MAIL = "mailto:support@oxy.ai";

/**
 * Stripe Checkout cancel landing page. Stripe redirects here when the
 * customer abandons checkout. Renders a simple message with a way to
 * retry (via the account team) or return to the org root.
 */
export default function CheckoutCancelledPage() {
  const { orgSlug = "" } = useParams<{ orgSlug: string }>();
  const navigate = useNavigate();

  return (
    <div className='flex min-h-[60vh] w-full items-center justify-center px-6 py-16'>
      <div className='w-full max-w-md space-y-6 rounded-2xl border bg-card p-8 text-center'>
        <div className='space-y-2'>
          <h1 className='font-serif text-2xl tracking-tight'>Checkout cancelled</h1>
          <p className='text-muted-foreground text-sm'>
            No subscription was created. You can retry from the email link, or contact your account
            team for a new checkout link.
          </p>
        </div>
        <div className='space-y-2'>
          <Button asChild className='w-full'>
            <a href={SUPPORT_MAIL}>Contact account team</a>
          </Button>
          <Button
            variant='outline'
            className='w-full'
            onClick={() => navigate(ROUTES.ORG(orgSlug).ROOT, { replace: true })}
          >
            Back to workspace
          </Button>
        </div>
      </div>
    </div>
  );
}
