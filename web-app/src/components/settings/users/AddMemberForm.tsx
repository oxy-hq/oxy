import React, { useState } from "react";
import { useAddUserToOrg } from "@/hooks/api/users/useUserMutations";
import { Input } from "@/components/ui/shadcn/input";
import { Button } from "@/components/ui/shadcn/button";
import { Label } from "@/components/ui/shadcn/label";
import {
  Dialog,
  DialogTrigger,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogFooter,
  DialogClose,
} from "@/components/ui/shadcn/dialog";
import { toast } from "sonner";
import { useForm, Controller } from "react-hook-form";
import {
  Select,
  SelectTrigger,
  SelectValue,
  SelectContent,
  SelectItem,
} from "@/components/ui/shadcn/select";

interface AddMemberFormProps {
  organizationId: string;
}

const AddMemberForm: React.FC<AddMemberFormProps> = ({ organizationId }) => {
  const [open, setOpen] = useState(false);
  const addUserMutation = useAddUserToOrg();
  const {
    register,
    handleSubmit,
    formState: { errors },
    reset,
    control,
  } = useForm<{ email: string; role: string }>({
    defaultValues: { email: "", role: "member" },
  });

  const onSubmit = async (data: { email: string; role: string }) => {
    try {
      await addUserMutation.mutateAsync({
        organizationId,
        email: data.email,
        role: data.role,
      });
      reset();
      toast.success("User added successfully");
      setOpen(false);
    } catch (err) {
      if (err instanceof Error && err.message.includes("404")) {
        toast.error("User not found");
      } else {
        toast.error("Failed to add user");
      }
    }
  };

  return (
    <Dialog open={open} onOpenChange={setOpen}>
      <DialogTrigger asChild>
        <Button size="sm">Add Member</Button>
      </DialogTrigger>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Add New Member</DialogTitle>
        </DialogHeader>
        <form className="space-y-4" onSubmit={handleSubmit(onSubmit)}>
          <div className="space-y-2">
            <Label htmlFor="email">Email</Label>
            <Input
              id="email"
              type="email"
              {...register("email", {
                required: "Email is required",
                pattern: {
                  value: /^[A-Z0-9._%+-]+@[A-Z0-9.-]+\.[A-Z]{2,}$/i,
                  message: "Invalid email address",
                },
              })}
            />
            {errors.email && (
              <span className="text-red-500 text-xs">
                {errors.email.message}
              </span>
            )}
          </div>
          <div className="space-y-2">
            <Label htmlFor="role">Role</Label>
            <Controller
              name="role"
              control={control}
              rules={{ required: true }}
              render={({ field }) => (
                <Select value={field.value} onValueChange={field.onChange}>
                  <SelectTrigger id="role">
                    <SelectValue placeholder="Select role" />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem value="member">Member</SelectItem>
                    <SelectItem value="admin">Admin</SelectItem>
                  </SelectContent>
                </Select>
              )}
            />
            {errors.role && (
              <span className="text-red-500 text-xs">Role is required</span>
            )}
          </div>
          <DialogFooter>
            <DialogClose asChild>
              <Button type="button" variant="outline">
                Cancel
              </Button>
            </DialogClose>
            <Button
              type="submit"
              variant="default"
              disabled={addUserMutation.status === "pending"}
            >
              {addUserMutation.status === "pending" ? "Adding..." : "Add"}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
};

export default AddMemberForm;
