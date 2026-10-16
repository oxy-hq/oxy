import type { ComponentProps } from "react";
import { Link } from "react-router-dom";
import { appWindowFor } from "./utils";

type FollowLinkProps = Omit<ComponentProps<"a">, "href" | "target"> & { href: string };

/**
 * Where a notification points: a custom app in its own named tab (plain anchor,
 * no `rel` — see `appWindowName`), a platform path through the router.
 */
export function FollowLink({ href, children, ...props }: FollowLinkProps) {
  const target = appWindowFor(href);
  if (target) {
    return (
      <a href={href} target={target} {...props}>
        {children}
        <span className='sr-only'> (opens in a new tab)</span>
      </a>
    );
  }
  return (
    <Link to={href} {...props}>
      {children}
    </Link>
  );
}
