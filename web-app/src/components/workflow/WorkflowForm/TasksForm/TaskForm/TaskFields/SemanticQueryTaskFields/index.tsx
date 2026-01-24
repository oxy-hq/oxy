import React from "react";
import { SemanticQueryTaskFieldsProps } from "./types";
import { useSemanticQueryFields } from "./useSemanticQueryFields";
import { TopicField } from "./TopicField";
import { DimensionsField } from "./DimensionsField";
import { MeasuresField } from "./MeasuresField";
import { FiltersField } from "./FiltersField";
import { OrdersField } from "./OrdersField";
import { LimitOffsetFields } from "./LimitOffsetFields";

export const SemanticQueryTaskFields: React.FC<
  SemanticQueryTaskFieldsProps
> = ({ index, basePath = "tasks" }) => {
  const {
    register,
    control,
    taskPath,
    taskErrors,
    topicValue,
    topicsLoading,
    topicsError,
    fieldsLoading,
    topicItems,
    dimensionItems,
    measureItems,
    allFieldItems,
  } = useSemanticQueryFields(index, basePath);

  return (
    <div className="space-y-4">
      <TopicField
        taskPath={taskPath}
        control={control}
        register={register}
        topicItems={topicItems}
        topicsLoading={topicsLoading}
        topicsError={topicsError}
        taskErrors={taskErrors}
      />

      <DimensionsField
        taskPath={taskPath}
        control={control}
        topicValue={topicValue}
        fieldsLoading={fieldsLoading}
        dimensionItems={dimensionItems}
      />

      <MeasuresField
        taskPath={taskPath}
        control={control}
        topicValue={topicValue}
        fieldsLoading={fieldsLoading}
        measureItems={measureItems}
      />

      <FiltersField
        taskPath={taskPath}
        control={control}
        register={register}
        topicValue={topicValue}
        fieldsLoading={fieldsLoading}
        allFieldItems={allFieldItems}
      />

      <OrdersField
        taskPath={taskPath}
        control={control}
        topicValue={topicValue}
        fieldsLoading={fieldsLoading}
        allFieldItems={allFieldItems}
      />

      <LimitOffsetFields taskPath={taskPath} register={register} />
    </div>
  );
};
