import { Input } from "@/components/ui/shadcn/input";
import { Label } from "@/components/ui/shadcn/label";
import { RadioGroup, RadioGroupItem } from "@/components/ui/shadcn/radio-group";
import { WarehousesFormData } from ".";
import { useFormContext } from "react-hook-form";
import { Info } from "lucide-react";

export default function SnowflakeForm({ index }: { index: number }) {
  const {
    formState: { errors },
    register,
    watch,
    setValue,
  } = useFormContext<WarehousesFormData>();

  const configErrors = errors?.warehouses?.[index]?.config as
    | Record<string, { message?: string }>
    | undefined;

  const authMode =
    (watch(`warehouses.${index}.config.auth_mode`) as string) || "password";

  return (
    <div className="space-y-4">
      {/* Auth Mode Selection */}
      <div className="space-y-2">
        <Label>Authentication Method</Label>
        <RadioGroup
          value={authMode as string}
          onValueChange={(value) => {
            setValue(`warehouses.${index}.config.auth_mode`, value);
            // Clear password when switching to browser auth
            if (value === "browser") {
              setValue(`warehouses.${index}.config.password`, undefined);
            }
          }}
          className="flex flex-col space-y-2"
        >
          <div className="flex items-center space-x-2">
            <RadioGroupItem value="password" id={`password-auth-${index}`} />
            <Label
              htmlFor={`password-auth-${index}`}
              className="font-normal cursor-pointer"
            >
              Password Authentication
            </Label>
          </div>
          <div className="flex items-center space-x-2">
            <RadioGroupItem value="browser" id={`browser-auth-${index}`} />
            <Label
              htmlFor={`browser-auth-${index}`}
              className="font-normal cursor-pointer"
            >
              Browser Authentication (SSO)
            </Label>
          </div>
        </RadioGroup>
        {authMode === "browser" && (
          <div className="flex items-start gap-2 p-3 bg-blue-50 dark:bg-blue-950 border border-blue-200 dark:border-blue-800 rounded-md">
            <Info className="h-4 w-4 mt-0.5 text-blue-600 dark:text-blue-400 flex-shrink-0" />
            <p className="text-xs text-blue-700 dark:text-blue-300">
              Browser authentication will open Snowflake SSO in your default
              browser. You only need to provide account, username, and warehouse
              details.
            </p>
          </div>
        )}
      </div>

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

      {/* Password field - only show for password auth */}
      {authMode === "password" && (
        <div className="space-y-2">
          <Label htmlFor={`password-${index}`}>Password</Label>
          <Input
            id={`password-${index}`}
            type="password"
            placeholder="••••••••"
            {...register(`warehouses.${index}.config.password`, {
              required:
                authMode === "password" ? "Password is required" : false,
            })}
          />
          {configErrors?.password && (
            <p className="text-xs text-destructive mt-1">
              {configErrors.password.message?.toString()}
            </p>
          )}
        </div>
      )}

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
