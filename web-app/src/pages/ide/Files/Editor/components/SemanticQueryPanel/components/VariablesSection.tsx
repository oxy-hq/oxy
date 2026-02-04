import type { Variable } from "..";
import VariableRow from "./VariableRow";

interface VariablesSectionProps {
  variables: Variable[];
  onUpdateVariable: (index: number, updates: Partial<Variable>) => void;
  onRemoveVariable: (index: number) => void;
}

const VariablesSection = ({
  variables,
  onUpdateVariable,
  onRemoveVariable
}: VariablesSectionProps) => {
  if (variables.length === 0) return null;

  return (
    <div className='space-y-2 border-b p-3'>
      <div>Variables</div>
      {variables.map((variable, index) => (
        <VariableRow
          key={index}
          variable={variable}
          onUpdate={(updates) => onUpdateVariable(index, updates)}
          onRemove={() => onRemoveVariable(index)}
        />
      ))}
    </div>
  );
};

export default VariablesSection;
