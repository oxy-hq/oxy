import {
  ReactNode,
  useState,
  createContext,
  useContext,
  useEffect,
} from "react";
import { css, cx } from "styled-system/css";
import Text from "@/components/ui/Typography/Text";

interface TabsProps {
  children: ReactNode;
  defaultValue: string;
  onChange?: (value: string) => void;
}

interface TabListProps {
  children: ReactNode;
  className?: string;
}

interface TabProps {
  children: ReactNode;
  value: string;
  isSelected?: boolean;
  onClick?: () => void;
}

interface TabPanelProps {
  children: ReactNode;
  value: string;
  selectedValue?: string;
  className?: string;
}

interface TabsContextType {
  selectedTab: string;
  setSelectedTab: (value: string) => void;
}

const TabsContext = createContext<TabsContextType | null>(null);

const useTabsContext = () => {
  const context = useContext(TabsContext);
  if (!context) {
    throw new Error("Tab components must be used within a Tabs component");
  }
  return context;
};

const tabListStyles = css({
  display: "flex",
  gap: "sm",
});

const tabStyles = css({
  padding: "sm",
  borderRadius: "sm",
  cursor: "pointer",
  backgroundColor: "transparent",
  border: "none",
  color: "neutral.text.colorTextSecondary",
  display: "flex",
  alignItems: "center",
  justifyContent: "center",
  height: "30px",
  _hover: {
    backgroundColor: "neutral.bg.colorBgHover",
  },
});

const selectedTabStyles = css({
  backgroundColor: "neutral.bg.colorBgActive",
  color: "neutral.text.colorText!",
});

export const TabList = ({ children, className }: TabListProps) => {
  return <div className={cx(tabListStyles, className)}>{children}</div>;
};

export const Tab = ({ children, value }: TabProps) => {
  const { selectedTab, setSelectedTab } = useTabsContext();
  const isSelected = value === selectedTab;

  return (
    <button
      className={cx(isSelected && selectedTabStyles, tabStyles)}
      onClick={() => setSelectedTab(value)}
    >
      <Text variant="tabBase">{children}</Text>
    </button>
  );
};

export const TabPanel = ({ children, value, className }: TabPanelProps) => {
  const { selectedTab } = useTabsContext();
  if (value !== selectedTab) return null;
  return <div className={className}>{children}</div>;
};

export const Tabs = ({ children, defaultValue, onChange }: TabsProps) => {
  const [selectedTab, setSelectedTab] = useState(defaultValue);

  useEffect(() => {
    if (onChange) {
      onChange(selectedTab);
    }
  }, [selectedTab, onChange]);

  return (
    <TabsContext.Provider value={{ selectedTab, setSelectedTab }}>
      {children}
    </TabsContext.Provider>
  );
};
