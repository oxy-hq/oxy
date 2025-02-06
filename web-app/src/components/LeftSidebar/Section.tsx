"use client";

import { css, cx } from "styled-system/css";

import Icon from "../ui/Icon";
import { SvgAssets } from "../ui/Icon/Dictionary";
import Text from "../ui/Typography/Text";

const headerStyles = css({
  display: "flex",
  p: "sm",
  alignItems: "center",
  justifyContent: "space-between",
});

const textStyles = css({
  display: "flex",
  gap: "sm",
  color: "text.light",
  "&[data-active=true]": {
    color: "text.less-contrast",
  },
  alignItems: "center",
});

type Props = {
  section: string;
  iconAsset?: SvgAssets;
  isActive?: boolean;
};

export default function Section({ section, iconAsset, isActive }: Props) {
  return (
    <div className={cx(headerStyles)}>
      <div className={textStyles} data-active={isActive}>
        {!!iconAsset && <Icon asset={iconAsset} />}
        <Text variant="label14Regular" color="lessContrast">
          {section}
        </Text>
      </div>
    </div>
  );
}
