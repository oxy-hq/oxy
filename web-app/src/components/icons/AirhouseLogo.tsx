import useTheme from "@/stores/useTheme";

interface AirhouseLogoProps {
  className?: string;
}

export const AirhouseLogo: React.FC<AirhouseLogoProps> = ({ className }) => {
  const theme = useTheme((s) => s.theme);
  return (
    <img
      src={theme === "dark" ? "/airhouse-dark.svg" : "/airhouse-light.svg"}
      alt='Airhouse'
      className={className}
    />
  );
};
