import React from "react";

import { TextareaAutosizeProps } from "react-textarea-autosize";

type TextAreaOwnProps = {
  variant?: "default";
};

export type TextAreaElement = React.ElementRef<"textarea">;
export interface TextAreaProps
  extends TextareaAutosizeProps,
    React.RefAttributes<HTMLTextAreaElement>,
    TextAreaOwnProps {}

export type TextAreaRootElement = React.ElementRef<"div">;
export interface TextAreaRootProps
  extends React.ComponentPropsWithRef<"div">,
    TextAreaOwnProps {}

export type TextAreaSlotElement = React.ElementRef<"div">;
// eslint-disable-next-line @typescript-eslint/no-empty-object-type
export interface TextAreaSlotProps extends React.ComponentPropsWithRef<"div"> {}

export type TextAreaContextValue = TextAreaProps;
export const TextAreaContext = React.createContext<
  TextAreaContextValue | undefined
>(undefined);
