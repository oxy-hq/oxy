export const formatLogContent = (log: Record<string, unknown>): string => {
  if (typeof log === "string") {
    return log;
  }
  if (log.queries && Array.isArray(log.queries)) {
    const queries = log.queries as Array<{
      database?: string;
      is_verified?: boolean;
      query?: string;
      source?: string;
    }>;

    if (queries.length === 0) {
      return "No queries found";
    }

    return queries
      .map((queryItem, index) => {
        let result = `Query ${index + 1}:\n`;

        const tags = [];
        if (queryItem.database) {
          tags.push(`[DB: ${queryItem.database}]`);
        }
        if (typeof queryItem.is_verified === "boolean") {
          tags.push(
            `[${queryItem.is_verified ? "VERIFIED ✓" : "UNVERIFIED ✗"}]`,
          );
        }
        if (queryItem.source) {
          tags.push(`[${queryItem.source}]`);
        }

        if (tags.length > 0) {
          result += `${tags.join(" ")}\n\n`;
        }

        if (queryItem.query) {
          result += `${queryItem.query}`;
        }

        return result;
      })
      .join("\n\n---\n\n");
  }

  if (log.message) {
    return String(log.message);
  }

  if (log.content) {
    return String(log.content);
  }

  if (log.error) {
    return String(log.error);
  }

  return JSON.stringify(log, null, 2);
};
