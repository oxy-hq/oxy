import { Handle, type NodeProps, Position } from "@xyflow/react";
import {
  BG_COLORS,
  BORDER_COLORS,
  HANDLE_STYLE_HIDDEN,
  HANDLE_STYLE_VISIBLE,
  ICONS
} from "../constants";

export function ContextGraphNode({ data }: NodeProps) {
  const { label, type, opacity, showLeftHandle, showRightHandle } = data as {
    label: string;
    type: string;
    opacity?: number;
    showLeftHandle?: boolean;
    showRightHandle?: boolean;
  };
  const borderColor = BORDER_COLORS[type];
  const nodeOpacity = opacity ?? 1;

  return (
    <div
      style={{
        position: "relative",
        width: "fit-content",
        opacity: nodeOpacity,
        transform: `scale(${nodeOpacity > 0 ? 1 : 0.8})`,
        transition: "opacity 0.3s ease, transform 0.3s ease",
        pointerEvents: nodeOpacity === 0 ? "none" : "auto"
      }}
    >
      <Handle
        type='target'
        position={Position.Left}
        style={showLeftHandle ? HANDLE_STYLE_VISIBLE : HANDLE_STYLE_HIDDEN}
      />
      <Handle
        type='source'
        position={Position.Right}
        style={showRightHandle ? HANDLE_STYLE_VISIBLE : HANDLE_STYLE_HIDDEN}
      />
      <div
        className='flex cursor-pointer items-center gap-2 rounded-md px-3 py-1.5 transition-shadow hover:opacity-90'
        style={{
          border: `1.5px solid ${borderColor}`,
          background: BG_COLORS[type],
          color: borderColor
        }}
      >
        {ICONS[type]}
        <span className='whitespace-nowrap font-medium text-foreground text-xs'>{label}</span>
      </div>
    </div>
  );
}
