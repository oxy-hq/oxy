import React from "react";

import type { RecipeVariantProps } from "styled-system/css";

import { css, cva, cx } from "styled-system/css";

import { ModifierKey, ModifierKeyMap } from "@/libs/keyboard";

import Icon from "../Icon";
import { SvgAssets } from "../Icon/Dictionary";
import { Switch } from "../Switch";
import Text from "../Typography/Text";

const buttonStyles = cva({
  base: {
    height: "4xl",
    display: "inline-flex",
    alignItems: "center",

    cursor: "pointer",
    outline: "none",
    border: "none",
    borderRadius: "rounded",
    padding: "sm",
    color: "text.light",
    gap: "sm",
    w: "100%"
  },
  variants: {
    variant: {
      light: {
        _hover: {
          bgColor: "surface.secondary"
        }
      },
      dark: {
        color: "text.light",
        _hover: {
          bgColor: "surface.tertiary"
        }
      },
      secondary: {
        color: "text.primary",
        bg: "background.primary",
        "--shadow-primary": "shadows.primary",
        "--border-color": "colors.border.primary",
        // border and shadow
        shadow: "inset 0 0 0 1px var(--border-color), var(--shadow-primary)",
        _hover: {
          // border
          shadow: "inset 0 0 0 1px var(--border-color)"
        }
      }
    },
    kind: {
      button: {
        justifyContent: "space-between"
      },
      toggle: {
        justifyContent: "space-between"
      }
    }
  }
});

const shortcutStyles = css({
  color: "text.secondary",
  textStyle: "paragraph12Regular"
});

const contentStyles = css({
  display: "inline-flex",
  alignItems: "center",
  gap: "sm"
});

type BaseProps = {
  iconAsset?: SvgAssets;
  text: string;
  className?: string;
  actionKey?: string;
  modifierKeys?: ModifierKey[];
};

export type ButtonVariantProps = RecipeVariantProps<typeof buttonStyles>;

type ActionButtonProps = React.ButtonHTMLAttributes<HTMLButtonElement> &
  ButtonVariantProps &
  BaseProps;

export const ActionButton = React.forwardRef<HTMLButtonElement, ActionButtonProps>(
  (
    {
      iconAsset,
      text,
      className,
      variant = "light",
      kind = "button",
      modifierKeys,
      actionKey,
      ...props
    },
    ref
  ) => {
    let shortcut = actionKey;
    if (modifierKeys && modifierKeys.length > 0) {
      const modifiers = modifierKeys.map((key) => ModifierKeyMap[key]);
      shortcut = [...modifiers, shortcut].join(" + ");
    }
    return (
      <button
        data-functional
        ref={ref}
        className={cx(buttonStyles({ variant, kind }), className)}
        {...props}
      >
        <div className={contentStyles}>
          {!!iconAsset && <Icon asset={iconAsset} />}
          <Text variant='label14Regular'>{text}</Text>
        </div>
        {shortcut && <span className={shortcutStyles}>{shortcut}</span>}
      </button>
    );
  }
);

ActionButton.displayName = "Button";

export function ToggleButton({
  iconAsset,
  text,
  className,
  checked,
  onCheckedChange
}: BaseProps & {
  checked: boolean;
  onCheckedChange?: (checked: boolean) => void;
}) {
  const handleClick = () => {
    onCheckedChange?.(!checked);
  };
  return (
    <div
      className={cx(buttonStyles({ variant: "light", kind: "toggle" }), className)}
      onClick={handleClick}
      aria-label={text}
    >
      <div className={contentStyles}>
        {!!iconAsset && <Icon asset={iconAsset} />}
        <Text variant='label14Regular'>{text}</Text>
      </div>
      <Switch checked={checked} />
    </div>
  );
}

