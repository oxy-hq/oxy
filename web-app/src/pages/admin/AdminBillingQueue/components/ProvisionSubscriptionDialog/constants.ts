export type ProvisionMethod = "invoice" | "checkout";

interface MethodOption {
  value: ProvisionMethod;
  title: string;
  shortDescription: string;
  note: string;
  recommended?: boolean;
}

export const METHOD_OPTIONS: MethodOption[] = [
  {
    value: "checkout",
    title: "Via Checkout",
    shortDescription:
      "Send a Stripe Checkout link; the customer enters billing address, tax ID, and card themselves.",
    note: "All prices must share the same billing interval.",
    recommended: true
  },
  {
    value: "invoice",
    title: "Via Invoice",
    shortDescription:
      "Create the subscription directly and email an invoice for the customer to pay.",
    note: "Supports flexible billing — prices may use different intervals."
  }
];
