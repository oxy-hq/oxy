import type { Control, FieldErrors, UseFormRegister } from "react-hook-form";
import type { AutomationFormData } from "../../..";

export interface SemanticQueryTaskFieldsProps {
  index: number;
  basePath?: string;
}

export interface FieldItem {
  value: string;
  label: string;
  searchText?: string;
  dataType?: string;
}

interface BaseFieldProps {
  taskPath: string;
  control: Control<AutomationFormData>;
  topicValue: string | undefined;
  fieldsLoading: boolean;
}

export interface TopicFieldProps {
  taskPath: string;
  control: Control<AutomationFormData>;
  register: UseFormRegister<AutomationFormData>;
  topicItems: FieldItem[];
  topicsLoading: boolean;
  topicsError: Error | null;
  taskErrors: FieldErrors | undefined;
}

export interface DimensionsFieldProps extends BaseFieldProps {
  dimensionItems: FieldItem[];
}

export interface MeasuresFieldProps extends BaseFieldProps {
  measureItems: FieldItem[];
}

export interface FiltersFieldProps extends BaseFieldProps {
  allFieldItems: FieldItem[];
}

export interface OrdersFieldProps extends BaseFieldProps {
  allFieldItems: FieldItem[];
}

export interface LimitOffsetFieldsProps {
  taskPath: string;
  register: UseFormRegister<AutomationFormData>;
}
