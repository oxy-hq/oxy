const isMac =
  typeof window !== "undefined" && window.navigator.platform === "MacIntel";

export type ModifierKey = "Alt" | "Control" | "Meta" | "Shift";

const getKeyRepresentation = (mac: string, other: string) =>
  isMac ? mac : other;

export const ModifierKeyMap: Record<ModifierKey, string> = {
  Alt: getKeyRepresentation("⌥", "alt"),
  Control: getKeyRepresentation("⌃", "ctrl"),
  Meta: getKeyRepresentation("⌘", "ctrl"),
  Shift: "⇧",
};

export type ActionHotKeyType =
  | "CreateNewQuestion"
  | "GoToSettings"
  | "GoToSettingsByKeySequences"
  | "JumpToPrompt"
  | "MoveUp"
  | "MoveDown"
  | "OpenFocusItem"
  | "ClearSelection"
  | "DeleteFocusedChat"
  | "ExitPrompt";

interface Key {
  value: string | string[];
  text: string;
}

export const HotKeys: Record<ActionHotKeyType, Key> = {
  CreateNewQuestion: {
    value: "c",
    text: "C",
  },
  GoToSettings: {
    value: "g+s",
    text: "G + S",
  },
  GoToSettingsByKeySequences: {
    value: ["g", "s"],
    text: "G then S",
  },
  JumpToPrompt: {
    value: "/",
    text: "/",
  },
  MoveUp: {
    value: ["ArrowUp", "k"],
    text: "↑",
  },
  MoveDown: {
    value: ["ArrowDown", "j"],
    text: "↓",
  },
  OpenFocusItem: {
    value: "Enter",
    text: "Enter",
  },
  ClearSelection: {
    value: "Escape",
    text: "Escape",
  },
  DeleteFocusedChat: {
    value: [`Control+Backspace`, `Meta+Backspace`],
    text: "⌘+delete",
  },
  ExitPrompt: {
    value: "Escape",
    text: "esc",
  },
};
