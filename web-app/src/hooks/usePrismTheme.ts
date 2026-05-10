import { oneDark, oneLight } from "react-syntax-highlighter/dist/esm/styles/prism";

import useTheme from "@/stores/useTheme";

const usePrismTheme = () => (useTheme((s) => s.theme) === "dark" ? oneDark : oneLight);

export default usePrismTheme;
