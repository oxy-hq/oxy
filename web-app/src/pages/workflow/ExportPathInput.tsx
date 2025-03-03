import { useFormContext } from "react-hook-form";
import ExportFieldContainer from "./ExportFieldContainer";
import ExportFieldLabel from "./ExportFieldLabel";
import FormTextInput from "./FormTextInput";

const ExportPathInput = () => {
  const { register } = useFormContext();
  return (
    <ExportFieldContainer>
      <ExportFieldLabel>Path</ExportFieldLabel>
      <FormTextInput {...register("export.path")} />
    </ExportFieldContainer>
  );
};

export default ExportPathInput;
