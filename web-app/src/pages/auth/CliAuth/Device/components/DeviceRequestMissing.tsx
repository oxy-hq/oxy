import type React from "react";
import { Link } from "react-router-dom";
import { Button } from "@/components/ui/shadcn/button";
import { apiStatus } from "@/libs/apiError";
import MintPage from "../../components/MintPage";

interface Props {
  error: Error;
  onRetry: () => void;
}

/**
 * The request could not be shown. A 404 is the server's one answer for a code that was decided,
 * lapsed or never existed — the page can't tell which, so it names all three. Anything else
 * failed to load, and can be tried again.
 */
const DeviceRequestMissing: React.FC<Props> = ({ error, onRetry }) =>
  apiStatus(error) === 404 ? (
    <MintPage
      status='error'
      title='No request is waiting for this code'
      lead='It was approved or declined already, it lapsed (a request waits ten minutes), or the code is mistyped. Check the code oxyc printed, or run the oxyc command again for a new one.'
    >
      <Button asChild variant='outline' className='mt-6' size='sm'>
        <Link to='/cli-auth/device'>Enter another code</Link>
      </Button>
    </MintPage>
  ) : (
    <MintPage
      status='error'
      title="Couldn't load the request"
      lead='Nothing was approved. Try again; the request waits ten minutes from when oxyc asked.'
    >
      <Button variant='outline' className='mt-6' size='sm' onClick={onRetry}>
        Try again
      </Button>
    </MintPage>
  );

export default DeviceRequestMissing;
