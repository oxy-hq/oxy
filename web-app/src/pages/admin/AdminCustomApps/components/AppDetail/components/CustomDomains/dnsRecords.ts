import type { CustomDomain } from "@/types/apps";

export interface DnsRecord {
  type: "CNAME" | "TXT";
  name: string;
  /** Null when there is nothing to copy yet (no routing target configured). */
  value: string | null;
}

/**
 * The two records a customer creates before a domain serves: the CNAME that routes
 * traffic, and the TXT that proves they own the name. A null `cnameTarget` means ops
 * have not configured one; the row keeps its place with no value, so the table still
 * shows what is missing.
 */
export const dnsRecords = (domain: CustomDomain, cnameTarget: string | null): DnsRecord[] => [
  { type: "CNAME", name: domain.domain, value: cnameTarget?.trim() || null },
  { type: "TXT", name: domain.txt_name, value: domain.txt_value }
];

/**
 * A host as typed into the add box, trimmed and lower-cased; null when blank. The
 * server validates properly (400 `invalid_domain`) — this only stops a stray space or
 * capital from becoming a different-looking row.
 */
export const normalizeDomainInput = (raw: string): string | null => {
  const host = raw.trim().toLowerCase();
  return host === "" ? null : host;
};
