import type React from "react";
import { DimensionsField } from "./DimensionsField";
import { FiltersField } from "./FiltersField";
import { LimitOffsetFields } from "./LimitOffsetFields";
import { MeasuresField } from "./MeasuresField";
import { OrdersField } from "./OrdersField";
import { TimeDimensionsField } from "./TimeDimensionsField";
import { TopicField } from "./TopicField";
import type { SemanticQueryTaskFieldsProps } from "./types";
import { useSemanticQueryFields } from "./useSemanticQueryFields";

export const SemanticQueryTaskFields: React.FC<SemanticQueryTaskFieldsProps> = ({
  index,
  basePath = "tasks"
}) => {
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
    dimensionItemsWithTypes
  } = useSemanticQueryFields(index, basePath);

  return (
    <div className='space-y-4'>
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

      <TimeDimensionsField
        taskPath={taskPath}
        control={control}
        topicValue={topicValue}
        fieldsLoading={fieldsLoading}
        dimensionItems={dimensionItemsWithTypes}
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
