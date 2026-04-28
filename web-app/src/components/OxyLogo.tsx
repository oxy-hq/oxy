import useTheme from "@/stores/useTheme";

const OxyLogo = () => {
  const theme = useTheme((state) => state.theme);
  return <img src={theme === "dark" ? "/oxygen-dark.svg" : "/oxygen-light.svg"} alt='Oxygen' />;
};

export default OxyLogo;
