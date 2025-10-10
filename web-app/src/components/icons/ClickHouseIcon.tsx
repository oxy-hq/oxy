interface ClickHouseIconProps {
  className?: string;
  width?: number;
  height?: number;
}

export const ClickHouseIcon = ({
  className,
  width = 32,
  height = 32,
}: ClickHouseIconProps) => {
  return (
    <svg
      width={width}
      height={height}
      viewBox="0 0 24 24"
      fill="white"
      xmlns="http://www.w3.org/2000/svg"
      className={className}
    >
      <path d="M.285 0h2.087c.142 0 .285.142.285.285v23.43a.306.306 0 0 1-.285.285H.285C.143 24 0 23.905 0 23.715V.285C0 .142.142 0 .285 0m5.312 0h2.087c.142 0 .285.142.285.285v23.43a.306.306 0 0 1-.285.285H5.597a.306.306 0 0 1-.285-.285V.285C5.359.143 5.454 0 5.597 0m5.36 0h2.087c.142 0 .285.142.285.285v23.43a.306.306 0 0 1-.285.285h-2.087a.306.306 0 0 1-.285-.285V.285c0-.142.142-.285.285-.285m5.312 0h2.087c.142 0 .285.142.285.285v23.43a.306.306 0 0 1-.285.285h-2.087a.306.306 0 0 1-.285-.285V.285c0-.142.142-.285.285-.285m5.36 9.344h2.086c.142 0 .285.142.285.285v4.791a.306.306 0 0 1-.285.285h-2.087a.306.306 0 0 1-.285-.285V9.628c0-.142.142-.285.285-.285" />
    </svg>
  );
};

export default ClickHouseIcon;
