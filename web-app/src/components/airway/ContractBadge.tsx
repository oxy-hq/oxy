/**
 * One resource's source contract, as a compact badge for the run grid.
 *
 * Presentation only. The detail (version column, restatement window,
 * cursor lag, rewind) lives in the tooltip so the grid stays scannable —
 * see `contractDisplay.ts`.
 */

import type React from "react";

import { Badge } from "@/components/ui/shadcn/badge";
import { cn } from "@/libs/shadcn/utils";
import type { ResourceContract } from "@/services/api/airway";

import { contractTooltip, MUTABILITY_CLASS, MUTABILITY_LABEL, rewindHint } from "./contractDisplay";

type Props = {
  /**
   * `undefined` means the run's event stream carried no contract
   * information (a run recorded before contracts rode on `pipeline_plan`,
   * or a normalized child table, which is not a source resource). That is
   * NOT the same as a connector declaring nothing — that arrives as a
   * real contract with `mutability: "undeclared"` — so we render an
   * em dash and claim nothing.
   */
  contract?: ResourceContract;
};

const ContractBadge: React.FC<Props> = ({ contract }) => {
  if (!contract) {
    return (
      <span
        className='text-muted-foreground text-xs'
        title='This run carried no contract information for this row.'
      >
        —
      </span>
    );
  }

  const hint = rewindHint(contract);
  return (
    <span className='inline-flex items-center gap-1.5' title={contractTooltip(contract)}>
      <Badge variant='outline' className={cn(MUTABILITY_CLASS[contract.mutability])}>
        {MUTABILITY_LABEL[contract.mutability]}
      </Badge>
      {hint && <span className='text-muted-foreground text-xs tabular-nums'>{hint}</span>}
    </span>
  );
};

export default ContractBadge;
