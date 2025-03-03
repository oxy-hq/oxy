import useWorkflow, {
  ExportConfig,
  LoopSequentialTaskConfig,
  Node,
  TaskConfig,
  TaskConfigWithId,
  TaskType,
} from "@/stores/useWorkflow";
import { SideBarContainer } from "./SideBarContainer";
import SideBarStepHeader from "./SideBarStepHeader";
import { v4 as uuid } from "uuid";
import { TextFieldWithLabel } from "./TextFieldWithLabel";
import { Controller, FormProvider, useForm, useWatch } from "react-hook-form";
import StepDataContainer from "./StepDataContainer";
import ExportSection from "./ExportSection";
import { useEffect, useMemo, useState } from "react";
import Icon from "@/components/ui/Icon";
import Text from "@/components/ui/Typography/Text";
import Button from "@/components/ui/Button";
import { css } from "styled-system/css";
import { taskIconMap } from "./utils";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/Dropdown";
import { ListContainer, ValuesField } from "./ValuesComponents";

type FormValue = {
  name: string;
  values: string | string[];
  export?: ExportConfig;
};

export const LoopSequentialSidebar: React.FC<{ node: Node }> = ({ node }) => {
  const updateStep = useWorkflow((state) => state.updateTask);
  const saveWorkflow = useWorkflow((state) => state.saveWorkflow);
  const task = node.data.task as LoopSequentialTaskConfig;
  const id = task.id;

  const defaultValues: FormValue = {
    name: task.name,
    values: task.values,
    export: task.export,
  };

  const methods = useForm<FormValue>({
    mode: "onChange",
    defaultValues,
  });

  const { register, control } = methods;
  const formValues = useWatch({ control: methods.control }) as FormValue;

  useEffect(() => {
    const updateData: Partial<LoopSequentialTaskConfig> = {
      name: formValues.name,
      values: formValues.values,
      export: formValues.export,
    };

    updateStep(id, updateData);
    saveWorkflow();
  }, [formValues, updateStep, saveWorkflow, id]);

  const onAddTask = (type: TaskType) => {
    let newTask: TaskConfig;
    switch (type) {
      case TaskType.EXECUTE_SQL:
        newTask = {
          id: uuid(),
          type: TaskType.EXECUTE_SQL,
          name: "Execute SQL",
          database: "",
          sql: "",
        };
        break;
      case TaskType.LOOP_SEQUENTIAL:
        newTask = {
          id: uuid(),
          type: TaskType.LOOP_SEQUENTIAL,
          name: "Loop Sequential",
          values: [],
          tasks: [],
        };
        break;
      case TaskType.FORMATTER:
        newTask = {
          id: uuid(),
          type: TaskType.FORMATTER,
          name: "Formatter",
          template: "",
        };
        break;
      case TaskType.AGENT:
        newTask = {
          id: uuid(),
          type: TaskType.AGENT,
          name: "Agent",
          prompt: "",
          agent_ref: "",
        };
        break;
    }
    updateStep(id, {
      tasks: [...task.tasks, newTask],
    });
    saveWorkflow();
  };

  return (
    <SideBarContainer>
      <SideBarStepHeader>Loop Sequential</SideBarStepHeader>
      <FormProvider {...methods}>
        <form>
          <StepDataContainer>
            <TextFieldWithLabel label="Name" {...register("name")} />
            <Controller
              name="values"
              control={control}
              render={({ field }) => {
                return <ValuesField {...field} />;
              }}
            ></Controller>
          </StepDataContainer>
          <TaskList tasks={task.tasks} onAddTask={onAddTask} />
          <ExportSection />
        </form>
      </FormProvider>
    </SideBarContainer>
  );
};

type TaskListProps = {
  tasks: TaskConfig[];
  onAddTask: (type: TaskType) => void;
};

const TaskList = ({ tasks, onAddTask }: TaskListProps) => {
  return (
    <ListContainer>
      <TaskListHeader title="Tasks" onAddTask={onAddTask} />
      <TaskListContent tasks={tasks} />
    </ListContainer>
  );
};

type TaskListContentProps = {
  tasks: TaskConfig[];
};

const TaskListContent = ({ tasks }: TaskListContentProps) => {
  return (
    <div
      className={css({
        display: "flex",
        flexDirection: "column",
        gap: "gap.gapXS",
      })}
    >
      {tasks.map((task) => (
        <TaskItem task={task} />
      ))}
    </div>
  );
};

type TaskListHeaderProps = {
  title: string;
  onAddTask: (type: TaskType) => void;
};

const TaskListHeader = ({ title, onAddTask }: TaskListHeaderProps) => {
  const [isAddMenuOpen, setIsAddMenuOpen] = useState(false);
  return (
    <div
      className={css({
        padding: "16px",
        display: "flex",
        justifyContent: "space-between",
        alignItems: "center",
      })}
    >
      <Text variant="panelTitle" weight="regular">
        {title}
      </Text>
      <AddTaskButton
        onOpenChange={setIsAddMenuOpen}
        isOpen={isAddMenuOpen}
        onAddTask={onAddTask}
      />
    </div>
  );
};

type AddTaskButtonProps = {
  isOpen: boolean;
  onOpenChange: (isOpen: boolean) => void;
  onAddTask: (type: TaskType) => void;
};

const AddTaskButton = ({
  isOpen,
  onOpenChange,
  onAddTask,
}: AddTaskButtonProps) => {
  return (
    <DropdownMenu open={isOpen} onOpenChange={onOpenChange}>
      <DropdownMenuTrigger asChild>
        <Button content="icon" variant="ghost" data-functional>
          <Icon asset="add" />
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent
        data-functional
        align="end"
        side="bottom"
        className={css({ w: "180px" })}
      >
        <DropdownMenuItem
          iconAsset="code"
          text="Execute SQL"
          onSelect={() => onAddTask(TaskType.EXECUTE_SQL)}
        />
        <DropdownMenuItem
          iconAsset="arrow_reload"
          text="Loop Sequential"
          onSelect={() => onAddTask(TaskType.LOOP_SEQUENTIAL)}
        />
        <DropdownMenuItem
          iconAsset="placeholder"
          text="Formatter"
          onSelect={() => onAddTask(TaskType.FORMATTER)}
        />
        <DropdownMenuItem
          iconAsset="agent"
          text="Agent"
          onSelect={() => onAddTask(TaskType.AGENT)}
        />
      </DropdownMenuContent>
    </DropdownMenu>
  );
};

type TaskItemProps = {
  task: TaskConfigWithId;
};

const TaskItem = ({ task }: TaskItemProps) => {
  const setSelectedNodeId = useWorkflow((state) => state.setSelectedNodeId);
  const selectedNodeId = useWorkflow((state) => state.selectedNodeId)!;
  const getAllParentIds = useWorkflow((state) => state.getAllParentIds);
  const removeStep = useWorkflow((state) => state.removeTask);
  const saveWorkflow = useWorkflow((state) => state.saveWorkflow);
  const isSelected = useMemo(() => {
    const parentIds = getAllParentIds(selectedNodeId);
    return selectedNodeId === task.id || parentIds.has(task.id);
  }, [getAllParentIds, selectedNodeId, task.id]);

  const onStepClick = () => {
    setSelectedNodeId(task.id);
  };

  const onRemoveStepClick = () => {
    removeStep(task.id);
    saveWorkflow();
  };

  const backgroundColor = isSelected ? "neutral.bg.colorBgActive" : "#FFF";
  return (
    <div
      className={css({
        display: "flex",
        justifyContent: "space-between",
        alignItems: "center",
        paddingLeft: "padding.padding",
        paddingRight: "padding.padding",
        gap: "10px",
      })}
    >
      <Button
        variant="filled"
        type="button"
        onClick={onStepClick}
        className={css({
          padding: "sm",
          border: "1px solid",
          backgroundColor: backgroundColor,
          borderColor: "neutral.200",
          borderRadius: "borderRadiusMS",
          flex: 1,
          display: "flex",
          alignItems: "center",
          gap: "sm",
        })}
      >
        <Icon asset={taskIconMap[task.type]} />
        <Text variant="body" size="base" weight="regular">
          {task.name}
        </Text>
      </Button>
      <Button variant="ghost" type="button" onClick={onRemoveStepClick}>
        <Icon asset="remove_minus" />
      </Button>
    </div>
  );
};
