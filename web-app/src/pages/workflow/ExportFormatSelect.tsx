import { Controller, useFormContext } from "react-hook-form";
import ExportFieldContainer from "./ExportFieldContainer";
import ExportFieldLabel from "./ExportFieldLabel";
import Select from "./Select";

const exportFormatOptions = [
  { label: ".sql", value: "sql" },
  { label: ".csv", value: "csv" },
  { label: ".json", value: "json" },
  { label: ".txt", value: "txt" },
  { label: ".docx", value: "docx" },
];

const ExportFormatSelect: React.FC = () => {
  const { control } = useFormContext();

  return (
    <ExportFieldContainer>
      <ExportFieldLabel>Format</ExportFieldLabel>
      <Controller
        name="export.format"
        control={control}
        render={({ field }) => (
          <Select
            {...field}
            options={exportFormatOptions}
            onValueChange={field.onChange}
          />
        )}
      />
    </ExportFieldContainer>
  );
};

export default ExportFormatSelect;
