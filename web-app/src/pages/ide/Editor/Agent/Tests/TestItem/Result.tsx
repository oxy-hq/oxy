import ReactMarkdown from "react-markdown";
import { useState } from "react";
import { ChevronRight } from "lucide-react";
import { TestResult } from "@/stores/useTests";

const Result = ({ result }: { result: TestResult }) => {
  const [showResult, setShowResult] = useState(false);
  const { metrics, records } = result;
  const inconsistencies = records.filter((record) => record.score < 1.0);

  const haveInconsistencies = inconsistencies.length > 0;

  return (
    <div className="px-1 rounded-b-lg">
      <div
        className="flex gap-2 py-1 px-2 items-center cursor-pointer"
        onClick={() => {
          if (haveInconsistencies) {
            setShowResult(!showResult);
          }
        }}
      >
        <ChevronRight
          className={`w-4 h-4 transition-transform duration-300 ease-in-out ${
            showResult ? "rotate-90" : "rotate-0"
          }`}
        />
        <p className="text-sidebar-foreground">Accuracy score:</p>
        <p className="text-green-500">{metrics.Accuracy}</p>
      </div>

      {haveInconsistencies && (
        <div
          className={`pl-8 transition-all duration-300 ease-in-out overflow-hidden ${
            showResult ? "max-h-[5000px] opacity-100" : "max-h-0 opacity-0"
          }`}
        >
          {inconsistencies.map((record) => (
            <div
              key={record.cot}
              className="py-2 text-muted-foreground font-mono text-sm"
            >
              <p className="py-2">Inconsistencies were found</p>

              <ReactMarkdown
                components={{
                  code: ({ ...props }) => {
                    const text = String(props.children);
                    const coloredText = text.replace(
                      /(\+\+\+|---)/g,
                      (match) =>
                        `<span class="${match === "+++" ? "text-green-500" : "text-red-500"}">${match}</span>`,
                    );
                    return (
                      <code
                        className="whitespace-pre-wrap break-words"
                        dangerouslySetInnerHTML={{ __html: coloredText }}
                      />
                    );
                  },
                  pre: ({ ...props }) => (
                    <pre
                      {...props}
                      className="whitespace-pre-wrap break-words overflow-x-auto"
                    />
                  ),
                }}
              >
                {`\`\`\`\n${record.cot}\n\`\`\``}
              </ReactMarkdown>
            </div>
          ))}
        </div>
      )}
    </div>
  );
};

export default Result;
