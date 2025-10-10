interface DuckDBIconProps {
  className?: string;
  width?: number;
  height?: number;
}

export const DuckDBIcon = ({
  className,
  width = 32,
  height = 32,
}: DuckDBIconProps) => {
  return (
    <svg
      width={width}
      height={height}
      viewBox="0 0 24 24"
      fill="white"
      xmlns="http://www.w3.org/2000/svg"
      className={className}
    >
      <path
        fill-rule="evenodd"
        clip-rule="evenodd"
        d="M12 24A11.985 11.985 0 0 1 0 12C0 5.361 5.361 0 12 0s12 5.361 12 12c0 6.632-5.361 12-12 12m6.417-13.794h-2.355v3.552h2.355c.983 0 1.785-.81 1.785-1.787 0-.975-.802-1.766-1.785-1.766m-8.915 6.76a4.98 4.98 0 0 1-4.974-4.974A4.98 4.98 0 0 1 9.502 7.02a4.98 4.98 0 0 1 4.973 4.972 4.98 4.98 0 0 1-4.974 4.974"
      />
    </svg>
  );
};

export default DuckDBIcon;
