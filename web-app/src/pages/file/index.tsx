import { useParams } from "react-router-dom";
import { css } from "styled-system/css";
import { Highlight, themes } from "prism-react-renderer";
import { FixedSizeList } from "react-window";
import Text from "@/components/ui/Typography/Text";
import useProjectPath from "@/stores/useProjectPath";

const readTextFile = async (path: string) => {
  const [handle] = await window.showOpenFilePicker({
    suggestedName: path,
  });
  const file = await handle.getFile();
  return await file.text();
};
import { useCallback, useEffect, useState, useMemo, useRef } from "react";

const styles = {
  wrapper: css({
    width: "100%",
    height: "100%",
    display: "flex",
    flexDir: "column",
  }),

  header: css({
    padding: "sm",
    border: "1px solid",
    borderColor: "neutral.border.colorBorderSecondary",
    backgroundColor: "neutral.bg.colorBg",
  }),

  codeContainer: css({
    height: "100%",
    backgroundColor: "neutral.bg.colorBgContainer",
  }),

  error: css({
    color: "red.500",
  }),
};

const LINE_HEIGHT = 21;

const FilePage = () => {
  const pathb64 = useParams<{ pathb64: string }>().pathb64!;
  const filePath = atob(pathb64);
  const projectPath = useProjectPath((state) => state.projectPath);
  const relativePath = filePath.replace(projectPath, "").replace(/^\//, "");

  const [fileContent, setFileContent] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [isLoading, setIsLoading] = useState(true);

  const lines = useMemo(() => fileContent.split("\n"), [fileContent]);

  const getFileContent = useCallback(async () => {
    try {
      setIsLoading(true);
      const content = await readTextFile(filePath);
      setFileContent(content);
      setError(null);
    } catch (err) {
      setError(
        `Failed to read file: ${err instanceof Error ? err.message : String(err)}`,
      );
      setFileContent("");
    } finally {
      setIsLoading(false);
    }
  }, [filePath]);

  useEffect(() => {
    getFileContent();
  }, [getFileContent]);

  const containerRef = useRef<HTMLDivElement>(null);
  const [containerHeight, setContainerHeight] = useState(0);

  const updateHeight = useCallback(() => {
    if (containerRef.current) {
      setContainerHeight(containerRef.current.clientHeight);
    }
  }, [containerRef]);

  useEffect(() => {
    updateHeight();
    window.addEventListener("resize", updateHeight);
    return () => window.removeEventListener("resize", updateHeight);
  }, [filePath, updateHeight]);

  const LineRenderer = useCallback(
    ({ index, style }: { index: number; style: React.CSSProperties }) => {
      const line = lines[index] || "";
      return (
        <div style={style}>
          <Highlight theme={themes.vsLight} code={line} language="typescript">
            {({
              className,
              style: highlightStyle,
              tokens,
              getLineProps,
              getTokenProps,
            }) => (
              <pre
                className={className}
                style={{
                  ...highlightStyle,
                  margin: 0,
                  padding: 0,
                  fontSize: "14px",
                }}
              >
                <div {...getLineProps({ line: tokens[0] })}>
                  <span style={{ color: "#666", marginRight: "1em" }}>
                    {index + 1}
                  </span>
                  {tokens[0].map((token, key) => (
                    <span key={key} {...getTokenProps({ token })} />
                  ))}
                </div>
              </pre>
            )}
          </Highlight>
        </div>
      );
    },
    [lines],
  );

  const renderContent = () => {
    if (isLoading) {
      return <Text variant="bodyBaseMedium">Loading...</Text>;
    }

    if (error) {
      return (
        <Text className={styles.error} variant="bodyBaseMedium">
          {error}
        </Text>
      );
    }

    return (
      <FixedSizeList
        height={containerHeight - 10}
        width="100%"
        itemCount={lines.length}
        itemSize={LINE_HEIGHT}
        overscanCount={20}
        className={css({
          customScrollbar: true,
          overflow: "overlay", // Better scrolling performance
          willChange: "transform",
          paddingBottom: "sm",
        })}
      >
        {LineRenderer}
      </FixedSizeList>
    );
  };

  return (
    <div className={styles.wrapper}>
      <div className={styles.header}>
        <Text variant="bodyBaseMedium">{relativePath}</Text>
      </div>
      <div className={styles.codeContainer} ref={containerRef}>
        {renderContent()}
      </div>
    </div>
  );
};

export default FilePage;
