/**
 * Opens a secure window to install a GitHub app
 * @param url The URL to open
 * @returns Window reference or null if blocked
 */
export const openSecureWindow = (url: string): Window | null => {
  const dualScreenLeft = window.screenLeft ?? window.screenX;
  const dualScreenTop = window.screenTop ?? window.screenY;
  const windowWidth = window.innerWidth ?? document.documentElement.clientWidth;
  const windowHeight = window.innerHeight ?? document.documentElement.clientHeight;

  // Calculate centered position
  const left = dualScreenLeft + (windowWidth - 600) / 2;
  const top = dualScreenTop + (windowHeight - 700) / 2;
  const features = ["popup=yes", "width=600", "height=700", `left=${left}`, `top=${top}`].join(",");

  const newWindow = window.open(url, "_blank", features);
  if (newWindow) {
    newWindow.focus(); // Bring popup to front
  }
  return newWindow;
};
