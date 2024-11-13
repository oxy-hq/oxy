import useTheme from "@/stores/useTheme";

export const AgentWithBg = ({ width }: { width?: number }) => {
  const { theme } = useTheme();
  const isDarkMode = theme === "dark";
  const src = isDarkMode ? "/onyx-light.svg" : "/onyx-dark.svg";

  return <img width={width ?? 100} src={src} alt='Logo' />;
};

