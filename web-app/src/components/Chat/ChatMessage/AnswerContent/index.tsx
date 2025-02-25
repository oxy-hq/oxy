"use client";

import { memo } from "react";

import Markdown, { ExtendedComponents } from "react-markdown";
import directive from "remark-directive";
import remarkGfm from "remark-gfm";
import { css, sva } from "styled-system/css";
import { stack } from "styled-system/patterns";

import CodeContainer from "./Code";

type Props = {
  content: string;
  className?: string;
};

const wrapperStyles = css({
  borderRadius: "borderRadiusXL",
  bg: "neutral.fill.colorFillTertiary",
  p: "padding.paddingSM",
});

const markdownStyles = sva({
  slots: ["root", "ol", "ul", "table", "thead", "th", "td", "tableWrap"],
  base: {
    root: stack.raw({
      textStyle: "paragraph14Regular",
      color: "text.primary",
      gap: "md",
      flexDirection: "column",
      "& > ol": {
        paddingInlineStart: "4xl",
        listStyleType: "decimal",
      },
      "& > ul": {
        paddingInlineStart: "4xl",
        listStyleType: "disc",
      },
      "& > ol > li > p": {
        display: "inline",
      },
    }),
    tableWrap: {
      rounded: "minimal",
      borderWidth: "1px",
      borderColor: "neutral.text.colorTextSecondary",
      overflow: "auto",
      "&::-webkit-scrollbar": {
        bg: "transparent",
        borderTop: "1px solid token(colors.neutral.text.colorTextSecondary)",
        height: "22px",
      },
      "&::-webkit-scrollbar-thumb": {
        bg: "neutral.text.colorTextSecondary",
        backgroundClip: "content-box",
        border: "8px solid transparent",
        borderRadius: "100px",
      },
    },
    table: {
      rounded: "minimal",
      borderWidth: "1px",
      borderColor: "neutral.text.colorTextSecondary",
      borderCollapse: "collapse",
      borderStyle: "hidden",
      width: "100%",
    },
    thead: {
      backgroundColor: "surface.secondary",
    },
    th: {
      minW: "140px",
      color: "text.primary",
      pl: "md",
      pr: "md",
      pt: "sm",
      pb: "sm",
      textAlign: "start !important",
    },
    td: {
      minW: "140px",
      color: "text.light",
      pl: "md",
      pr: "md",
      pt: "sm",
      pb: "sm",
      textAlign: "start !important",
      borderWidth: "1px",
      borderColor: "neutral.text.colorTextSecondary",
      borderCollapse: "collapse",
    },
  },
});

const extendedComponents: ExtendedComponents = {
  table: ({ children, ...props }) => {
    const classes = markdownStyles();
    return (
      <div className={classes.tableWrap}>
        <table className={classes.table} {...props}>
          {children}
        </table>
      </div>
    );
  },
  thead: ({ children, ...props }) => {
    const classes = markdownStyles();
    return (
      <thead className={classes.thead} {...props}>
        {children}
      </thead>
    );
  },
  th: ({ children, ...props }) => {
    const classes = markdownStyles();
    return (
      <th className={classes.th} {...props}>
        {children}
      </th>
    );
  },
  td: ({ children, ...props }) => {
    const classes = markdownStyles();
    return (
      <td className={classes.td} {...props}>
        {children}
      </td>
    );
  },
  code: (props) => <CodeContainer {...props} />,
};

// Basic component, need to override default styles
function AnswerContent({ content }: Props) {
  const classes = markdownStyles();
  return (
    <div className={wrapperStyles}>
      <div className={classes.root}>
        <Markdown
          remarkPlugins={[directive, remarkGfm]}
          components={extendedComponents}
        >
          {content}
        </Markdown>
      </div>
    </div>
  );
}

export default memo(AnswerContent);
