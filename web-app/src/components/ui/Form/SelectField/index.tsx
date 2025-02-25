"use client";

import * as React from "react";
import * as SelectPrimitive from "@radix-ui/react-select";
import { cx, RecipeVariantProps } from "styled-system/css";
import Icon from "@/components/ui/Icon";
import { selectFieldStyles } from "./SelectField.styles";
import { SvgAssets } from "../../Icon/Dictionary";

type SelectFieldStyledProps = RecipeVariantProps<typeof selectFieldStyles>;
// eslint-disable-next-line sonarjs/redundant-type-aliases
type SelectFieldContextValue = SelectFieldStyledProps;

const SelectContext = React.createContext<SelectFieldContextValue | undefined>(
  undefined,
);

type SelectFieldProps = SelectPrimitive.SelectProps & SelectFieldStyledProps;

const SelectRoot: React.FC<SelectFieldProps> = (props) => {
  const { children, state = "default", ...rootProps } = props;
  const contextValue = React.useMemo(() => ({ state }), [state]);

  return (
    <SelectPrimitive.Root {...rootProps}>
      <SelectContext.Provider value={contextValue}>
        {children}
      </SelectContext.Provider>
    </SelectPrimitive.Root>
  );
};

SelectRoot.displayName = "SelectRoot";

const SelectTrigger = React.forwardRef<
  React.ElementRef<typeof SelectPrimitive.Trigger>,
  React.ComponentPropsWithoutRef<typeof SelectPrimitive.Trigger>
>(({ className, children, ...props }, ref) => {
  const context = React.useContext(SelectContext);

  const styles = selectFieldStyles({
    state: context?.state ?? "default",
  });

  return (
    <SelectPrimitive.Trigger
      role="select-trigger"
      className={cx(styles.trigger, className)}
      ref={ref}
      {...props}
    >
      {children}
      <SelectPrimitive.Icon className={styles.icon} asChild>
        <Icon size="default" asset="chevron_down" />
      </SelectPrimitive.Icon>
    </SelectPrimitive.Trigger>
  );
});

SelectTrigger.displayName = "SelectTrigger";

const SelectValue = React.forwardRef<
  React.ElementRef<typeof SelectPrimitive.Value>,
  React.ComponentPropsWithoutRef<typeof SelectPrimitive.Value>
>(({ className, children, ...props }, ref) => {
  const context = React.useContext(SelectContext);

  const styles = selectFieldStyles({
    state: context?.state || "default",
  });

  return (
    <SelectPrimitive.Value
      className={cx(styles.value, className)}
      ref={ref}
      {...props}
    >
      {children}
    </SelectPrimitive.Value>
  );
});
SelectValue.displayName = SelectPrimitive.Value.displayName;

const SelectContent = React.forwardRef<
  React.ElementRef<typeof SelectPrimitive.Content>,
  React.ComponentPropsWithoutRef<typeof SelectPrimitive.Content>
>(({ className, children, position = "popper", ...props }, ref) => {
  const context = React.useContext(SelectContext);

  const styles = selectFieldStyles({
    state: context?.state || "default",
  });

  return (
    <SelectPrimitive.Portal container={document.getElementById("app-root")}>
      <SelectPrimitive.Content
        className={cx(styles.content, className)}
        ref={ref}
        position={position}
        {...props}
      >
        <SelectPrimitive.Viewport className={cx(styles.viewport)}>
          {children}
        </SelectPrimitive.Viewport>
      </SelectPrimitive.Content>
    </SelectPrimitive.Portal>
  );
});

SelectContent.displayName = "SelectContent";

const SelectItem = React.forwardRef<
  React.ElementRef<typeof SelectPrimitive.Item>,
  React.ComponentPropsWithoutRef<typeof SelectPrimitive.Item> & {
    icon?: SvgAssets;
  }
>(({ className, children, icon, ...props }, ref) => {
  const context = React.useContext(SelectContext);

  const styles = selectFieldStyles({
    state: context?.state || "default",
  });

  return (
    <SelectPrimitive.Item
      className={cx(styles.item, className)}
      ref={ref}
      {...props}
    >
      {icon && <Icon asset={icon} />}
      <SelectPrimitive.ItemText>{children}</SelectPrimitive.ItemText>
    </SelectPrimitive.Item>
  );
});

SelectItem.displayName = "SelectItem";

export { SelectRoot, SelectValue, SelectTrigger, SelectContent, SelectItem };
