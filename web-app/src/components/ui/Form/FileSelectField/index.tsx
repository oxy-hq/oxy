"use client";

import React, { useRef, useState } from "react";
import { cx, RecipeVariantProps, sva, css } from "styled-system/css";
import useMergedRefs from "@/hooks/utils/useMergedRefs";
import Text from "@/components/ui/Typography/Text";
import Icon from "@/components/ui/Icon";
import { open } from "@tauri-apps/plugin-dialog";

const removeIconButton = css({
  cursor: "pointer",
  _hover: {
    color: "text.primary",
  },
});

const fileSelectFieldInputStyles = sva({
  slots: ["root", "input", "icon", "placeholder", "value", "removeBtn"],
  base: {
    root: {
      display: "flex",
      py: "sm",
      px: "md",
      // TODO: Confirm with design if this is the max width for form fields
      maxW: "420px",
      alignItems: "center",
      borderRadius: "rounded",
      flexShrink: "0",
      height: "4xl",
      width: "100%",
      justifyContent: "center",
      cursor: "pointer",
      outline: "none",
      backgroundColor: "rgba(0, 0, 0, 0.02)",
      border: "1px dashed #E5E5E5",

      "&[aria-disabled=true]": {
        color: "text.secondary",
        // border
        shadow: "inset 0 0 0 1px token(colors.border.primary)",
        bg: "background.primary",
        _hover: {
          // border
          shadow: "inset 0 0 0 1px token(colors.border.primary)",
        },
      },
    },
    input: {
      display: "none",
    },
    icon: {},
    removeBtn: {
      display: "flex",
      alignItems: "center",
    },
  },
  variants: {
    state: {
      default: {
        root: {
          bg: "surface.secondary",
          color: "text.secondary",
          _focus: {
            // border
            shadow: "inset 0 0 0 1px token(colors.border.light)",
            color: "text.primary",
          },
          _hover: {
            // border
            shadow: "inset 0 0 0 1px token(colors.border.light)",
          },
        },
        placeholder: {
          color: "text.secondary",
        },
        value: {
          color: "text.primary",
          textStyle: "label14Regular",
          truncate: true,
        },
      },
      error: {
        root: {
          bg: "background.secondary",
          color: "text.primary",
          // border and shadow
          shadow:
            "inset 0 0 0 1px token(colors.border.error), token(shadows.error)",
          _focus: {
            // border
            shadow: "inset 0 0 0 1px token(colors.border.light)",
          },
        },
        placeholder: {
          color: "text.secondary",
        },
        value: {
          color: "text.primary",
        },
      },
    },
  },
});

type FileSelectFieldStyledProps = RecipeVariantProps<
  typeof fileSelectFieldInputStyles
>;
type FileSelectFieldInputElement = React.ElementRef<"input">;
type FileSelectFieldInputProps = React.ComponentPropsWithRef<"input"> &
  FileSelectFieldStyledProps & {
    basePath: string;
  };

const FileSelectField = React.forwardRef<
  FileSelectFieldInputElement,
  FileSelectFieldInputProps
>((props, forwardedRef) => {
  const {
    className,
    disabled,
    state = "default",
    placeholder = "Select a file...",
    onChange,
    basePath = "",
    defaultValue,
    ...inputProps
  } = props;
  const [fileName, setFileName] = useState(defaultValue);
  const localRef = useRef<HTMLInputElement>(null);
  const fileInputRef = useMergedRefs(localRef, forwardedRef);
  const styles = fileSelectFieldInputStyles({ state });

  const handleRemove = (e: React.MouseEvent<HTMLButtonElement>) => {
    e.preventDefault();
    e.stopPropagation();

    if (localRef.current) {
      localRef.current.value = "";
    }
    setFileName("");

    if (onChange) {
      onChange({
        target: {
          value: null,
        },
      } as unknown as React.ChangeEvent<HTMLInputElement>);
    }
  };

  const handleClick = () => {
    open({ multiple: false, defaultPath: basePath })
      .then((result) => {
        if (!result) return;
        const relativePath = (result as string)
          .replace(basePath, "")
          .replace(/^\//, "");
        setFileName(relativePath);
        if (onChange) {
          onChange({
            target: { value: relativePath, name: props.name },
          } as unknown as React.ChangeEvent<HTMLInputElement>);
        }
        return true;
      })
      .catch((error) => {
        console.error("Failed to open file dialog", error);
      });
  };

  return (
    <label
      tabIndex={disabled ? undefined : 0}
      className={styles.root}
      aria-disabled={disabled}
      onClick={handleClick}
    >
      {fileName ? (
        <Text className={styles.value} variant="label14Regular">
          {fileName}
        </Text>
      ) : (
        <Text className={styles.placeholder} variant="label14Regular">
          {placeholder}
        </Text>
      )}
      {fileName && (
        <button
          className={styles.removeBtn}
          aria-label="Remove File"
          onClick={handleRemove}
        >
          <Icon className={removeIconButton} asset="trash" />
        </button>
      )}
      <input
        {...inputProps}
        disabled
        ref={fileInputRef}
        // onChange={handleChange}
        className={cx(styles.input, "file-select-field-input", className)}
        value={fileName}
      />
    </label>
  );
});

FileSelectField.displayName = "FileSelectField";

export default FileSelectField;
