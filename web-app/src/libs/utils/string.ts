export const capitalize = (string: string) =>
  string.charAt(0).toUpperCase() + string.slice(1);

export const getAgentNameFromPath = (path: string) => {
  const parts = path.split("/");
  parts[parts.length - 1] = parts[parts.length - 1].split(".")[0];
  return parts.join(" - ");
};
