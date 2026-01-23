import { useState } from "react";
import { Button } from "@/components/ui/shadcn/button";
import { Input } from "@/components/ui/shadcn/input";
import { cn } from "@/libs/utils/cn";
import { FormProvider, useFieldArray, useForm } from "react-hook-form";
import { Trash2, Loader2 } from "lucide-react";
import { toast } from "sonner";

import BigQueryForm from "@/pages/create-workspace/steps/WarehouseStep/BigQueryForm";
import DuckDBForm from "@/pages/create-workspace/steps/WarehouseStep/DuckDBForm";
import SnowflakeForm from "@/pages/create-workspace/steps/WarehouseStep/SnowflakeForm";
import PostgresForm from "@/pages/create-workspace/steps/WarehouseStep/PostgresForm";
import RedshiftForm from "@/pages/create-workspace/steps/WarehouseStep/RedshiftForm";
import MysqlForm from "@/pages/create-workspace/steps/WarehouseStep/MysqlForm";
import ClickHouseForm from "@/pages/create-workspace/steps/WarehouseStep/ClickHouseForm";
import { TestConnectionSection } from "./TestConnectionSection";
import {
  BigQueryIcon,
  SnowflakeIcon,
  PostgresIcon,
  RedshiftIcon,
  MysqlIcon,
  DuckDBIcon,
  ClickHouseIcon,
} from "@/components/icons";
import {
  WarehousesFormData,
  DatabaseConfigType,
  WarehouseConfig,
} from "@/types/database";
import { useCreateDatabaseConfig } from "@/hooks/api/databases/useCreateDatabaseConfig";
import { useTestDatabaseConnection } from "@/hooks/api/databases/useTestDatabaseConnection";

interface WarehouseOption {
  type: DatabaseConfigType;
  label: string;
  icon?: React.ReactNode;
}

const warehouseOptions: WarehouseOption[] = [
  {
    type: "bigquery",
    label: "BigQuery",
    icon: <BigQueryIcon />,
  },
  {
    type: "snowflake",
    label: "Snowflake",
    icon: <SnowflakeIcon />,
  },
  {
    type: "postgres",
    label: "PostgreSQL",
    icon: <PostgresIcon />,
  },
  {
    type: "redshift",
    label: "Redshift",
    icon: <RedshiftIcon />,
  },
  {
    type: "mysql",
    label: "MySQL",
    icon: <MysqlIcon />,
  },
  {
    type: "duckdb",
    label: "DuckDB",
    icon: <DuckDBIcon />,
  },
  {
    type: "clickhouse",
    label: "ClickHouse",
    icon: <ClickHouseIcon />,
  },
];

interface AddDatabaseFormProps {
  onSuccess: () => void;
  onCancel: () => void;
}

export function AddDatabaseForm({ onSuccess, onCancel }: AddDatabaseFormProps) {
  const createMutation = useCreateDatabaseConfig();
  const testConnection = useTestDatabaseConnection();

  // Track which warehouses have been tested successfully
  const [currentTestingIndex, setCurrentTestingIndex] = useState<number | null>(
    null,
  );
  // Track which warehouse's results are currently being displayed
  const [displayResultsForIndex, setDisplayResultsForIndex] = useState<
    number | null
  >(null);

  const methods = useForm<WarehousesFormData>({
    defaultValues: {
      warehouses: [
        {
          type: "bigquery",
          name: "BIGQUERY_1",
          config: {},
        },
      ],
    },
  });

  const { register, control, handleSubmit, watch, setValue, reset } = methods;

  const { fields, remove } = useFieldArray({
    control,
    name: "warehouses",
  });

  const handleTypeChange = (index: number, value: DatabaseConfigType) => {
    setValue(`warehouses.${index}.type`, value);
    setValue(`warehouses.${index}.config`, {});
  };

  const handleTestConnection = async (index: number) => {
    const warehouse = watch(`warehouses.${index}`) as WarehouseConfig;

    if (!warehouse.config || Object.keys(warehouse.config).length === 0) {
      toast.error("Please fill in the connection details first");
      return;
    }

    setCurrentTestingIndex(index);
    setDisplayResultsForIndex(index);
    testConnection.reset();

    try {
      await testConnection.testConnection({ warehouse });

      // Check if the test completed successfully
      // The hook will update its state, we'll check it in the render
    } catch (error) {
      toast.error("Connection test failed", {
        description: error instanceof Error ? error.message : "Unknown error",
      });
    } finally {
      setCurrentTestingIndex(null);
    }
  };

  const onSubmit = async (data: WarehousesFormData) => {
    try {
      await createMutation.mutateAsync(data);
      reset();
      setDisplayResultsForIndex(null);
      testConnection.reset();
      onSuccess();
    } catch (error) {
      // Error is already handled in the mutation
      console.error("Failed to create database config:", error);
      toast.error("Failed to create database configuration");
    }
  };

  const handleCancel = () => {
    reset();
    setCurrentTestingIndex(null);
    setDisplayResultsForIndex(null);
    testConnection.reset();
    onCancel();
  };

  const handleRemove = (index: number) => {
    remove(index);
  };

  const renderWarehouseForm = (
    index: number,
    warehouseType: DatabaseConfigType,
  ) => {
    switch (warehouseType) {
      case "bigquery":
        return <BigQueryForm index={index} />;
      case "duckdb":
        return <DuckDBForm index={index} />;
      case "snowflake":
        return <SnowflakeForm index={index} />;
      case "postgres":
        return <PostgresForm index={index} />;
      case "redshift":
        return <RedshiftForm index={index} />;
      case "mysql":
        return <MysqlForm index={index} />;
      case "clickhouse":
        return <ClickHouseForm index={index} />;
      default:
        return null;
    }
  };

  return (
    <div className="space-y-6">
      <FormProvider {...methods}>
        <form onSubmit={handleSubmit(onSubmit)} className="space-y-6">
          <div className="space-y-4">
            {fields.map((field, index) => {
              const warehouseType = watch(
                `warehouses.${index}.type`,
              ) as DatabaseConfigType;

              const defaultName = `${warehouseType.toUpperCase()}_${index + 1}`;
              const nameError =
                methods.formState.errors?.warehouses?.[index]?.name;

              const isTesting =
                currentTestingIndex === index && testConnection.isLoading;
              const showTestResult = displayResultsForIndex === index;

              return (
                <div
                  key={field.id}
                  className={cn(
                    "bg-muted/40 border rounded-md p-3 gap-4 flex flex-col",
                  )}
                >
                  <div>
                    <div className="flex flex-row items-center justify-between gap-4">
                      <div className="flex items-center gap-2 flex-1">
                        <Input
                          {...register(`warehouses.${index}.name`, {
                            required: "Connection name is required",
                            validate: {
                              unique: (value) => {
                                const warehouseNames = methods
                                  .getValues("warehouses")
                                  .map((warehouse, i) =>
                                    i !== index ? warehouse.name : null,
                                  );
                                return (
                                  !warehouseNames.includes(value) ||
                                  "Warehouse name must be unique"
                                );
                              },
                            },
                          })}
                          defaultValue={defaultName}
                          placeholder="Connection Name"
                        />
                      </div>
                      {fields.length > 1 && (
                        <Button
                          type="button"
                          variant="ghost"
                          size="icon"
                          onClick={() => handleRemove(index)}
                          className="h-8 w-8"
                        >
                          <Trash2 className="h-4 w-4" />
                        </Button>
                      )}
                    </div>
                    {nameError && (
                      <p className="text-xs text-destructive mt-1">
                        {nameError.message?.toString()}
                      </p>
                    )}
                  </div>

                  <div className="flex flex-wrap gap-4">
                    {warehouseOptions.map((option) => (
                      <Button
                        key={option.type}
                        type="button"
                        variant="outline"
                        size="icon"
                        onClick={() => handleTypeChange(index, option.type)}
                        className={cn(
                          "px-8 py-4",
                          warehouseType === option.type && "border-primary",
                        )}
                      >
                        {option.icon}
                      </Button>
                    ))}
                  </div>

                  {renderWarehouseForm(index, warehouseType)}
                  <TestConnectionSection
                    isTesting={isTesting}
                    showTestResult={showTestResult}
                    testConnection={testConnection}
                    onTest={() => handleTestConnection(index)}
                    disabled={createMutation.isPending}
                  />
                </div>
              );
            })}
          </div>

          <div className="space-y-3">
            <div className="flex justify-end gap-2">
              <Button
                type="button"
                variant="outline"
                onClick={handleCancel}
                disabled={createMutation.isPending}
              >
                Cancel
              </Button>
              <Button type="submit" disabled={createMutation.isPending}>
                {createMutation.isPending ? (
                  <>
                    <Loader2 className="h-4 w-4 animate-spin" />
                    Creating...
                  </>
                ) : (
                  "Create"
                )}
              </Button>
            </div>
          </div>
        </form>
      </FormProvider>
    </div>
  );
}
