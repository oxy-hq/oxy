import { useState, useEffect } from "react";
import type { TableData } from "@oxy-hq/sdk";

interface DataPreviewModalProps {
  name: string;
  data: TableData;
  onClose: () => void;
}

export default function DataPreviewModal({
  name,
  data,
  onClose,
}: DataPreviewModalProps) {
  const [isFullscreen, setIsFullscreen] = useState(false);

  const toggleFullscreen = () => {
    setIsFullscreen(!isFullscreen);
  };

  const handleBackdropClick = (e: React.MouseEvent) => {
    if (e.target === e.currentTarget) {
      onClose();
    }
  };

  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        if (isFullscreen) {
          setIsFullscreen(false);
        } else {
          onClose();
        }
      } else if (e.key === "f" || e.key === "F") {
        toggleFullscreen();
      }
    };

    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [isFullscreen, onClose]);

  return (
    <div className="modal-backdrop" onClick={handleBackdropClick}>
      <div className={`modal-content ${isFullscreen ? "fullscreen" : ""}`}>
        <div className="modal-header">
          <div className="modal-title">
            <h3>{name}</h3>
            <div className="modal-subtitle">
              Showing {data.rows.length} of{" "}
              {data.total_rows || data.rows.length} rows
            </div>
          </div>
          <div className="modal-actions">
            <button
              onClick={toggleFullscreen}
              className="btn-icon"
              title={isFullscreen ? "Exit fullscreen" : "Fullscreen"}
            >
              {isFullscreen ? "⊡" : "⛶"}
            </button>
            <button
              onClick={onClose}
              className="btn-icon btn-close"
              title="Close"
            >
              ✕
            </button>
          </div>
        </div>

        <div className="modal-body">
          <div className="table-wrapper">
            <table className="data-table">
              <thead>
                <tr>
                  {data.columns.map((col, idx) => (
                    <th key={idx}>{col}</th>
                  ))}
                </tr>
              </thead>
              <tbody>
                {data.rows.map((row, rowIdx) => (
                  <tr key={rowIdx}>
                    {row.map((cell, cellIdx) => (
                      <td key={cellIdx}>
                        {cell === null || cell === undefined ? (
                          <span className="null-value">null</span>
                        ) : (
                          String(cell)
                        )}
                      </td>
                    ))}
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </div>
      </div>
    </div>
  );
}
