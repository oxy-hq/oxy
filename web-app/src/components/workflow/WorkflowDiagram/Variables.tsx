import { DialogTitle } from "@radix-ui/react-dialog";
import {
  buildCompleteYupSchema,
  createHeadlessForm,
  type JSONSchemaObjectType
} from "@remoteoss/json-schema-form";
import { useCallback, useMemo } from "react";
import { useForm } from "react-hook-form";
import { create } from "zustand";
import { Checkbox } from "@/components/ui/checkbox";
import { Button } from "@/components/ui/shadcn/button";
import { Dialog, DialogContent, DialogFooter, DialogHeader } from "@/components/ui/shadcn/dialog";
import {
  Field,
  FieldContent,
  FieldDescription,
  FieldError,
  FieldLabel,
  FieldTitle
} from "@/components/ui/shadcn/field";
import { Input } from "@/components/ui/shadcn/input";
import { RadioGroup, RadioGroupItem } from "@/components/ui/shadcn/radio-group";

type TData = Record<string, unknown>;

interface VariablesState {
  isOpen: boolean;
  onSubmit?: (data: TData) => Promise<unknown>;
  setIsOpen: (isOpen: boolean, onSubmit?: (data: TData) => Promise<unknown>) => void;
}

export const useVariables = create<VariablesState>()((set) => ({
  isOpen: false,
  onSubmit: undefined,
  setIsOpen: (isOpen: boolean, onSubmit) => set({ isOpen, onSubmit })
}));

type Props = {
  schema: JSONSchemaObjectType;
};

type YupValidationSchema = {
  validate: (data: TData, options: { abortEarly: boolean }) => Promise<TData>;
};

type YupErrors = {
  inner: {
    path: string;
    type: string;
    message: string;
  }[];
};

const useYupValidationResolver = (validationSchema: YupValidationSchema) =>
  useCallback(
    async (data: TData) => {
      try {
        const values = await validationSchema.validate(data, {
          abortEarly: false
        });

        return {
          values,
          errors: {}
        };
      } catch (errors) {
        return {
          values: {},
          errors: (errors as YupErrors).inner.reduce(
            (allErrors: Record<string, { type: string; message: string }>, currentError) => ({
              ...allErrors,
              [currentError.path]: {
                type: currentError.type ?? "validation",
                message: currentError.message
              }
            }),
            {}
          )
        };
      }
    },
    [validationSchema]
  );

export function Variables({ schema }: Props) {
  const { isOpen, onSubmit, setIsOpen } = useVariables();
  const { fields } = useMemo(
    () =>
      createHeadlessForm(schema, {
        strictInputType: false
      }),
    [schema]
  );
  const yupSchema = useMemo(
    () =>
      buildCompleteYupSchema(fields, {
        strictInputType: false
      }),
    [fields]
  );
  const yupResolver = useYupValidationResolver(yupSchema);
  const {
    handleSubmit,
    register,
    formState: { errors },
    reset,
    setError
  } = useForm({ resolver: yupResolver });
  const onClose = useCallback(() => {
    reset();
    setIsOpen(false, undefined);
  }, [reset, setIsOpen]);
  const onOpenChange = useCallback(
    (open: boolean) => {
      if (!open) {
        onClose();
      }
    },
    [onClose]
  );

  return (
    <Dialog open={isOpen} onOpenChange={onOpenChange}>
      <DialogContent className='sm:max-w-[425px]'>
        <DialogHeader>
          <DialogTitle>Run Automation With Variables</DialogTitle>
        </DialogHeader>
        <div className='flex h-full overflow-hidden'>
          <div className='customScrollbar scrollbar-gutter-auto flex-1 overflow-auto'>
            <form
              id='workflow-variables-form'
              onSubmit={handleSubmit(async (data) => {
                try {
                  await onSubmit?.(data);
                  onClose();
                } catch (error) {
                  setError("serverError", {
                    type: "server",
                    message: `${error}`
                  });
                }
              })}
            >
              {fields.map((field) => {
                const fieldName = field.name as string;
                const fieldType = field.type as string;
                let fieldInput;
                switch (fieldType) {
                  case "number":
                    fieldInput = (
                      <Input
                        type='number'
                        id={fieldName}
                        defaultValue={field.default as number}
                        step={"any"}
                        {...register(fieldName)}
                      />
                    );
                    break;
                  case "boolean":
                    fieldInput = (
                      <Checkbox
                        id={fieldName}
                        {...register(fieldName)}
                        defaultValue={field.default as string}
                      />
                    );
                    break;
                  case "radio": {
                    const options =
                      (field.options as {
                        label: string;
                        value: unknown;
                      }[]) || [];
                    fieldInput = (
                      <RadioGroup id={fieldName} {...register(fieldName)}>
                        {options.map(({ label, value }) => {
                          return (
                            <FieldLabel key={`${value}`} htmlFor={`radiogroup-${value}`}>
                              <Field orientation='horizontal'>
                                <FieldContent>
                                  <FieldTitle>{label}</FieldTitle>
                                </FieldContent>
                                <RadioGroupItem
                                  value={value as string}
                                  id={`radiogroup-${value}`}
                                />
                              </Field>
                            </FieldLabel>
                          );
                        })}
                      </RadioGroup>
                    );
                    break;
                  }
                  // Add more cases for different field types as needed
                  default:
                    fieldInput = (
                      <Input
                        id={fieldName}
                        {...register(fieldName)}
                        defaultValue={field.default as string}
                      />
                    );
                    break;
                }
                return (
                  <Field key={fieldName}>
                    <FieldLabel htmlFor={fieldName}>{fieldName}</FieldLabel>
                    {fieldInput}
                    {field.description ? (
                      <FieldDescription>{`${field.description}`}</FieldDescription>
                    ) : null}
                    <FieldError errors={[errors?.[fieldName]]} />
                  </Field>
                );
              })}
            </form>
          </div>
        </div>
        <DialogFooter>
          <FieldError errors={[errors?.serverError]} />
          <Button type='reset' variant='outline' onClick={onClose} className='mr-2'>
            Cancel
          </Button>
          <Button type='submit' form='workflow-variables-form'>
            Submit
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
