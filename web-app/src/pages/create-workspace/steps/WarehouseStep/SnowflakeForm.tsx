import { Input } from "@/components/ui/shadcn/input";
import { Label } from "@/components/ui/shadcn/label";
import { WarehousesFormData } from ".";
import { useFormContext } from "react-hook-form";

export default function SnowflakeForm({ index }: { index: number }) {
  const {
    formState: { errors },
    register,
  } = useFormContext<WarehousesFormData>();

  const configErrors = errors?.warehouses?.[index]?.config as
    | Record<string, { message?: string }>
    | undefined;

  return (
    <div className="space-y-4">
      <div className="space-y-2">
        <Label htmlFor={`account-${index}`}>Account</Label>
        <Input
          id={`account-${index}`}
          placeholder="your_account"
          {...register(`warehouses.${index}.config.account`, {
            required: "Account is required",
            pattern: {
              value: /^[a-zA-Z0-9_-]+$/,
              message: "Enter a valid account identifier",
            },
          })}
        />
        {configErrors?.account && (
          <p className="text-xs text-destructive mt-1">
            {configErrors.account.message?.toString()}
          </p>
        )}
      </div>

      <div className="space-y-2">
        <Label htmlFor={`username-${index}`}>Username</Label>
        <Input
          id={`username-${index}`}
          placeholder="username"
          {...register(`warehouses.${index}.config.username`, {
            required: "Username is required",
            pattern: {
              value: /^\w+$/,
              message: "Enter a valid username",
            },
          })}
        />
        {configErrors?.username && (
          <p className="text-xs text-destructive mt-1">
            {configErrors.username.message?.toString()}
          </p>
        )}
      </div>

      <div className="space-y-2">
        <Label htmlFor={`password-${index}`}>Password</Label>
        <Input
          id={`password-${index}`}
          type="password"
          placeholder="••••••••"
          {...register(`warehouses.${index}.config.password`, {
            required: "Password is required",
          })}
        />
        {configErrors?.password && (
          <p className="text-xs text-destructive mt-1">
            {configErrors.password.message?.toString()}
          </p>
        )}
      </div>

      <div className="space-y-2">
        <Label htmlFor={`warehouse-${index}`}>Warehouse</Label>
        <Input
          id={`warehouse-${index}`}
          placeholder="your_warehouse"
          {...register(`warehouses.${index}.config.warehouse`, {
            required: "Warehouse is required",
          })}
        />
        {configErrors?.warehouse && (
          <p className="text-xs text-destructive mt-1">
            {configErrors.warehouse.message?.toString()}
          </p>
        )}
      </div>

      <div className="space-y-2">
        <Label htmlFor={`database-${index}`}>Database</Label>
        <Input
          id={`database-${index}`}
          placeholder="your_database"
          {...register(`warehouses.${index}.config.database`, {
            required: "Database is required",
            pattern: {
              value: /^\w+$/,
              message: "Enter a valid database name",
            },
          })}
        />
        {configErrors?.database && (
          <p className="text-xs text-destructive mt-1">
            {configErrors.database.message?.toString()}
          </p>
        )}
      </div>

      <div className="space-y-2">
        <Label htmlFor={`role-${index}`}>Role (Optional)</Label>
        <Input
          id={`role-${index}`}
          placeholder="your_role"
          {...register(`warehouses.${index}.config.role`)}
        />
        {configErrors?.role && (
          <p className="text-xs text-destructive mt-1">
            {configErrors.role.message?.toString()}
          </p>
        )}
      </div>
    </div>
  );
}
