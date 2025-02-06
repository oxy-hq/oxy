import { hstack, vstack } from "styled-system/patterns";

import Button from "../ui/Button";
import Icon from "../ui/Icon";
import Text from "../ui/Typography/Text";

const Header = () => {
  const getGreeting = () => {
    const currentHour = new Date().getHours();
    if (currentHour < 12) {
      return "Good morning";
    } else if (currentHour < 18) {
      return "Good afternoon";
    } else {
      return "Good evening";
    }
  };

  return (
    <div className={hstack({ justify: "space-between" })}>
      <div className={vstack({ gap: "xs", alignItems: "start" })}>
        <Text variant="headline20Semibold" color="primary">
          Onyx Workspace
        </Text>
        <Text variant="paragraph16Regular" color="secondary">
          {getGreeting()}
        </Text>
      </div>
      <div
        className={hstack({
          gap: "xs",
          rowGap: "xs",
          flexWrap: "wrap",
          md: {
            flexWrap: "nowrap",
          },
        })}
      >
        <Button
          variant="filled"
          size="large"
          content="iconText"
          onClick={() => {
            window.open(
              "https://docs.onyxint.ai/welcome",
              "_blank",
              "noopener",
            );
          }}
        >
          <Icon asset="knowledge" />
          Docs
        </Button>
      </div>
    </div>
  );
};

export default Header;
