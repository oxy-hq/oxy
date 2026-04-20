// Extract a human-readable message from an API error, preferring the response body.
// For 409 Conflict, returns a specific duplicate-name message if the body isn't readable.
// Returns null (not the generic Axios "Request failed…" message) when nothing is available.
export function extractErrorMessage(err: unknown): string | null {
  if (!err || typeof err !== "object") return null;
  const response = (err as { response?: { data?: unknown; status?: number } })?.response;
  const body = response?.data;
  if (typeof body === "string" && body.length > 0) return body;
  if (response?.status === 409) {
    return "A workspace with that name already exists. Please choose a different name.";
  }
  return null;
}
