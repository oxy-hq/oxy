import Step from "./Step";
import { useAutoAnimate } from "@formkit/auto-animate/react";

const STEP_MAP = {
  execute_sql: "Execute SQL",
  visualize: "Generate visualization",
  retrieve: "Retrieve data",
};

const Steps = ({
  steps,
  isLoading,
}: {
  steps: string[];
  isLoading: boolean;
}) => {
  const [parent] = useAutoAnimate({
    duration: 300,
  });

  return (
    <div ref={parent}>
      {steps.length > 0 && isLoading && (
        <div>
          {steps.map((step, index) => (
            <Step
              key={step}
              title={STEP_MAP[step as keyof typeof STEP_MAP]}
              isCompleted={index < steps.length - 1}
            />
          ))}
        </div>
      )}
    </div>
  );
};

export default Steps;
