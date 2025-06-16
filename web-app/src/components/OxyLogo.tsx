import useTheme from "@/stores/useTheme";

const OxyLogo = () => {
  const theme = useTheme((state) => state.theme);
  return (
    <img
      src={theme === "dark" ? "/oxy-dark.svg" : "/oxy-light.svg"}
      alt="Oxy"
    />
  );
};

export default OxyLogo;
