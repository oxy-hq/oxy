import { Plus, Trash2 } from "lucide-react";
import type React from "react";
import { FormProvider, useFieldArray, useForm } from "react-hook-form";
import {
  BigQueryIcon,
  ClickHouseIcon,
  DuckDBIcon,
  MysqlIcon,
  PostgresIcon,
  RedshiftIcon,
  SnowflakeIcon
} from "@/components/icons";
import { Button } from "@/components/ui/shadcn/button";
import { Input } from "@/components/ui/shadcn/input";
import { cn } from "@/libs/utils/cn";
import Header from "../Header";
import BigQueryForm from "./BigQueryForm";
import ClickHouseForm from "./ClickHouseForm";
import DuckDBForm from "./DuckDBForm";
import MysqlForm from "./MysqlForm";
import PostgresForm from "./PostgresForm";
import RedshiftForm from "./RedshiftForm";
import SnowflakeForm from "./SnowflakeForm";

export type WarehouseType =
  | "bigquery"
  | "duckdb"
  | "snowflake"
  | "postgres"
  | "redshift"
  | "mysql"
  | "clickhouse";

interface WarehouseOption {
  type: WarehouseType;
  label: string;
  icon?: React.ReactNode;
}

interface DuckDBConfig {
  dataset: string;
}

interface BigQueryConfig {
  key: string;
  dataset: string;
  dry_run_limit?: number;
}

interface ClickHouseConfig {
  host: string;
  database: string;
  username: string;
  password: string;
}

interface SnowflakeConfig {
  account: string;
  username: string;
  password: string;
  warehouse: string;
  database: string;
  role?: string;
}

interface PostgresConfig {
  host: string;
  port: number;
  username: string;
  password: string;
  database: string;
}

interface RedshiftConfig {
  host: string;
  port: number;
  username: string;
  password: string;
  database: string;
}

interface MysqlConfig {
  host: string;
  port: number;
  username: string;
  password: string;
  database: string;
}

export type WarehouseConfig = {
  type: WarehouseType;
  name?: string;
  config:
    | BigQueryConfig
    | DuckDBConfig
    | SnowflakeConfig
    | PostgresConfig
    | RedshiftConfig
    | MysqlConfig
    | ClickHouseConfig
    | Record<string, unknown>;
};

export interface WarehousesFormData {
  warehouses: WarehouseConfig[];
}

const warehouseOptions: WarehouseOption[] = [
  {
    type: "bigquery",
    label: "BigQuery",
    icon: <BigQueryIcon />
  },
  {
    type: "snowflake",
    label: "Snowflake",
    icon: <SnowflakeIcon />
  },
  {
    type: "postgres",
    label: "PostgreSQL",
    icon: <PostgresIcon />
  },
  {
    type: "redshift",
    label: "Redshift",
    icon: <RedshiftIcon />
  },
  {
    type: "mysql",
    label: "MySQL",
    icon: <MysqlIcon />
  },
  {
    type: "duckdb",
    label: "DuckDB",
    icon: <DuckDBIcon />
  },
  {
    type: "clickhouse",
    label: "ClickHouse",
    icon: <ClickHouseIcon />
  }
];

interface WarehouseStepProps {
  initialData?: WarehousesFormData | null;
  onNext: (data: WarehousesFormData) => void;
  onBack: () => void;
}

export default function WarehouseStep({ initialData, onNext, onBack }: WarehouseStepProps) {
  const methods = useForm<WarehousesFormData>({
    defaultValues: initialData || {
      warehouses: [
        {
          type: "bigquery",
          name: "BIGQUERY_1",
          config: {}
        }
      ]
    }
  });

  const { register, control, handleSubmit, watch, setValue } = methods;

  const { fields, append, remove } = useFieldArray({
    control,
    name: "warehouses"
  });

  const handleTypeChange = (index: number, value: WarehouseType) => {
    setValue(`warehouses.${index}.type`, value);
    setValue(`warehouses.${index}.config`, {});
  };

  const onSubmit = (data: WarehousesFormData) => {
    onNext(data);
  };

  const renderWarehouseForm = (index: number, warehouseType: WarehouseType) => {
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
    <FormProvider {...methods}>
      <form onSubmit={handleSubmit(onSubmit)} className='space-y-6'>
        <div className='space-y-6'>
          <Header title='Add connections' description='Securely connect your data sources.' />

          {fields.map((field, index) => {
            const warehouseType = watch(`warehouses.${index}.type`) as WarehouseType;

            const defaultName = `${warehouseType.toUpperCase()}_${index + 1}`;
            const nameError = methods.formState.errors?.warehouses?.[index]?.name;

            return (
              <div
                key={field.id}
                className='mb-4 flex flex-col gap-4 rounded-md border bg-muted/40 p-3'
              >
                <div>
                  <div className='flex flex-row items-center justify-between gap-4'>
                    <Input
                      {...register(`warehouses.${index}.name`, {
                        required: "Connection name is required",
                        validate: {
                          unique: (value) => {
                            const warehouseNames = methods
                              .getValues("warehouses")
                              .map((warehouse, i) => (i !== index ? warehouse.name : null));
                            return (
                              !warehouseNames.includes(value) || "Warehouse name must be unique"
                            );
                          }
                        }
                      })}
                      defaultValue={defaultName}
                      placeholder='Connection Name'
                    />
                    {fields.length > 1 && (
                      <Button
                        type='button'
                        variant='ghost'
                        size='icon'
                        onClick={() => remove(index)}
                        className='h-8 w-8'
                      >
                        <Trash2 className='h-4 w-4' />
                      </Button>
                    )}
                  </div>
                  {nameError && (
                    <p className='mt-1 text-destructive text-xs'>{nameError.message?.toString()}</p>
                  )}
                </div>

                <div className='flex flex-wrap gap-4'>
                  {warehouseOptions.map((option) => (
                    <Button
                      key={option.type}
                      type='button'
                      variant='outline'
                      size='icon'
                      onClick={() => handleTypeChange(index, option.type)}
                      className={cn("px-8 py-4", warehouseType === option.type && "border-primary")}
                    >
                      {option.icon}
                    </Button>
                  ))}
                </div>

                {renderWarehouseForm(index, warehouseType)}
              </div>
            );
          })}

          <Button
            type='button'
            variant='outline'
            size='sm'
            onClick={() =>
              append({
                type: "bigquery",
                name: `BIGQUERY_${fields.length + 1}`,
                config: {}
              })
            }
            className='w-full'
          >
            <Plus className='h-4 w-4' /> Add Another Connection
          </Button>
        </div>

        <div className='flex justify-between'>
          <Button type='button' variant='outline' onClick={onBack}>
            Back
          </Button>
          <Button type='submit'>Next</Button>
        </div>
      </form>
    </FormProvider>
  );
}
