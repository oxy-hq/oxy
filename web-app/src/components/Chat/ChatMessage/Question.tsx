import { css } from "styled-system/css";
import Text from "../../ui/Typography/Text";

type Props = {
  content: string;
};

const containerStyles = css({
  width: "100%",
  display: "flex",
  justifyContent: "flex-end",
  maxW: {
    base: "350px",
    sm: "720px"
  },
  marginX: "auto"
});

const textWrapperStyles = css({
  paddingX: "md",
  paddingY: "sm",
  borderRadius: "rounded",
  bg: "surface.secondary",
  width: "fit-content",
  alignSelf: "flex-end"
});

const contentStyles = css({
  color: "text.light",
  wordBreak: "break-word"
});

function Question({ content }: Props) {
  return (
    <div className={containerStyles}>
      <div className={textWrapperStyles}>
        <Text as="p" variant="paragraph14Regular" className={contentStyles}>
          {content}
        </Text>
      </div>
    </div>
  );
}

export default Question;
