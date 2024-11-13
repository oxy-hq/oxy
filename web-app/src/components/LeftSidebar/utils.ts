export const handleHotkeys = (callback: () => void) => () => {
  if (
    typeof window !== "undefined" &&
    document.querySelector('[role="menu"], [role="dialog"]')
  ) {
    return;
  }
  callback();
};
