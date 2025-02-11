"use client";

import * as React from "react";

import * as DropdownMenuPrimitive from "@radix-ui/react-dropdown-menu";
import { css, cx } from "styled-system/css";

import { ActionButton, ToggleButton } from "../ActionButton";
import { SvgAssets } from "../Icon/Dictionary";
import Text from "../Typography/Text";

const DEFAULT_SIDE_OFFSET = 4;

const DropdownMenu = DropdownMenuPrimitive.Root;

const DropdownMenuTrigger = DropdownMenuPrimitive.Trigger;

const DropdownMenuGroup = DropdownMenuPrimitive.Group;

const DropdownMenuPortal = DropdownMenuPrimitive.Portal;

const DropdownMenuSub = DropdownMenuPrimitive.Sub;

const DropdownMenuRadioGroup = DropdownMenuPrimitive.RadioGroup;

const DropdownMenuSubTrigger = DropdownMenuPrimitive.SubTrigger;

const DropdownMenuSubContent = DropdownMenuPrimitive.SubContent;

const dropdownMenuContentStyles = css({
  display: "flex",
  flexDirection: "column",
  zIndex: 110,
  minWidth: "128px",
  overflow: "hidden",
  borderRadius: "rounded",
  // border and shadow
  shadow: "0 0 0 1px token(colors.border.primary), token(shadows.primary)",
  p: "sm",
  bgColor: "surface.primary",
});

const DropdownMenuContent = React.forwardRef<
  React.ElementRef<typeof DropdownMenuPrimitive.Content>,
  React.ComponentPropsWithoutRef<typeof DropdownMenuPrimitive.Content>
>(({ className, sideOffset = DEFAULT_SIDE_OFFSET, ...props }, ref) => (
  <DropdownMenuPrimitive.Portal
    container={document.getElementById("app-root")!}
  >
    <DropdownMenuPrimitive.Content
      ref={ref}
      sideOffset={sideOffset}
      className={cx(dropdownMenuContentStyles, className)}
      {...props}
    />
  </DropdownMenuPrimitive.Portal>
));
DropdownMenuContent.displayName = DropdownMenuPrimitive.Content.displayName;

const itemStyles = css({
  borderRadius: "rounded",
  _focusVisible: {
    outline: "none",
    boxShadow: "none",
  },
  _highlighted: {
    bgColor: {
      _light: "surface.secondary",
      _dark: "surface.tertiary",
    },
    color: "text.primary",
  },
  w: "100%",
});

const DropdownMenuItem = React.forwardRef<
  React.ElementRef<typeof DropdownMenuPrimitive.Item>,
  React.ComponentPropsWithoutRef<typeof DropdownMenuPrimitive.Item> & {
    inset?: boolean;
    iconAsset: SvgAssets;
    text: string;
    actionKey?: string;
    buttonClassName?: string;
  }
>(
  (
    { className, iconAsset, text, actionKey, buttonClassName, ...props },
    ref,
  ) => {
    return (
      <DropdownMenuPrimitive.Item
        ref={ref}
        className={cx(itemStyles, className)}
        {...props}
      >
        <ActionButton
          className={buttonClassName}
          iconAsset={iconAsset}
          text={text}
          actionKey={actionKey}
        />
      </DropdownMenuPrimitive.Item>
    );
  },
);

DropdownMenuItem.displayName = DropdownMenuPrimitive.Item.displayName;

const DropdownMenuCheckboxItem = React.forwardRef<
  React.ElementRef<typeof DropdownMenuPrimitive.CheckboxItem>,
  React.ComponentPropsWithoutRef<typeof DropdownMenuPrimitive.CheckboxItem> & {
    iconAsset: SvgAssets;
    text: string;
  }
>(({ className, checked, iconAsset, text, ...props }, ref) => (
  <DropdownMenuPrimitive.CheckboxItem
    ref={ref}
    className={cx(itemStyles, className)}
    checked={checked}
    {...props}
  >
    <ToggleButton
      checked={checked === true}
      iconAsset={iconAsset}
      text={text}
    />
  </DropdownMenuPrimitive.CheckboxItem>
));
DropdownMenuCheckboxItem.displayName =
  DropdownMenuPrimitive.CheckboxItem.displayName;

const DropdownMenuRadioItem = DropdownMenuPrimitive.RadioItem;

const labelStyles = css({
  display: "flex",
  flexDirection: "column",
  gap: "xs",
  p: "sm",
  color: "text.primary",
});

const DropdownMenuLabel = React.forwardRef<
  React.ElementRef<typeof DropdownMenuPrimitive.Label>,
  React.ComponentPropsWithoutRef<typeof DropdownMenuPrimitive.Label> & {
    title?: string;
    description?: string;
  }
>(({ className, title, description, ...props }, ref) => (
  <DropdownMenuPrimitive.Label
    ref={ref}
    className={cx(labelStyles, className)}
    {...props}
  >
    {title && <Text variant="label14Regular">{title}</Text>}
    {description && <Text variant="label12Regular">{description}</Text>}
  </DropdownMenuPrimitive.Label>
));
DropdownMenuLabel.displayName = DropdownMenuPrimitive.Label.displayName;

const separatorStyles = css({
  h: "1px",
  alignSelf: "stretch",
  bgColor: "border.primary",
  marginY: "sm",
});

const DropdownMenuSeparator = React.forwardRef<
  React.ElementRef<typeof DropdownMenuPrimitive.Separator>,
  React.ComponentPropsWithoutRef<typeof DropdownMenuPrimitive.Separator>
>(({ className, ...props }, ref) => (
  <DropdownMenuPrimitive.Separator
    ref={ref}
    className={cx(separatorStyles, className)}
    {...props}
  />
));
DropdownMenuSeparator.displayName = DropdownMenuPrimitive.Separator.displayName;

export {
  DropdownMenu,
  DropdownMenuTrigger,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuCheckboxItem,
  DropdownMenuRadioItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuGroup,
  DropdownMenuPortal,
  DropdownMenuSub,
  DropdownMenuSubContent,
  DropdownMenuSubTrigger,
  DropdownMenuRadioGroup,
};
