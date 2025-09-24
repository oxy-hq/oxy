import useTheme from "@/stores/useTheme";

const Header = () => {
  const { theme } = useTheme();

  return (
    <div className="border-b border-border p-4 mb-6">
      <div className="flex items-center gap-2">
        <img
          width={24}
          height={24}
          src={theme === "dark" ? "/oxy-dark.svg" : "/oxy-light.svg"}
          alt="Oxy"
        />
        <div className="text-border">
          <svg
            viewBox="0 0 24 24"
            width="16"
            height="16"
            stroke="currentColor"
            strokeWidth="1"
            strokeLinecap="round"
            strokeLinejoin="round"
            fill="none"
            shapeRendering="geometricPrecision"
          >
            <path d="M16 3.549L7.12 20.600" />
          </svg>
        </div>
        <p className="text-sm">Workspaces</p>
      </div>
    </div>
  );
};

export default Header;
