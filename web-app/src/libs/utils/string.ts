export const capitalize = (string: string) =>
  string.charAt(0).toUpperCase() + string.slice(1);

export const getAgentNameFromPath = (path: string) => {
  const parts = path.split("/");
  parts[parts.length - 1] = parts[parts.length - 1].split(".")[0];
  return parts.join(" - ");
};

// eslint-disable-next-line sonarjs/pseudo-random
export const randomKey = () => Math.random().toString(36).substring(2, 15);

export const getShortTitle = (message: string) => {
  const words = message.trim().split(/\s+/);
  const baseTitle = words.slice(0, 8).join(" ");
  let shortTitle = words.length > 8 ? baseTitle : message;

  if (shortTitle.length > 50) {
    shortTitle = shortTitle.slice(0, 50) + "...";
  } else if (shortTitle !== message) {
    shortTitle += "...";
  }

  return shortTitle;
};

export const handleDownloadFile = (
  blob: Blob | MediaSource,
  fileName: string,
) => {
  const url = window.URL.createObjectURL(blob);
  const a = document.createElement("a");
  a.href = url;
  a.download = fileName;
  document.body.appendChild(a);
  a.click();
  document.body.removeChild(a);
  window.URL.revokeObjectURL(url);
};
