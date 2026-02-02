import { ReactNode, createContext, useContext, useMemo } from "react";
import { useTopicDetails } from "@/hooks/api/useSemanticQuery";
import { TopicData, ViewWithData } from "../../types";
import { useEditorContext } from "../../contexts/useEditorContext";
import {
  SemanticExplorerProvider,
  useSemanticExplorerContext,
} from "../../contexts/SemanticExplorerContext";

type TopicExplorerProviderProps = {
  children: ReactNode;
};

type TopicExplorerContextType = {
  topicData: TopicData | null;
  viewsWithData: ViewWithData[];
  topicLoading: boolean;
  loadingTopicError: string | undefined;
  refetchTopicDetails: () => void;
};

const TopicExplorerContext = createContext<TopicExplorerContextType | null>(
  null,
);

const TopicExplorerProviderInner = ({
  children,
}: TopicExplorerProviderProps) => {
  const { pathb64 } = useEditorContext();

  const {
    data: topicDetails,
    isLoading: topicLoading,
    error: loadingTopicError,
    refetch: refetchTopicDetails,
  } = useTopicDetails(pathb64);

  const viewsWithData = useMemo<ViewWithData[]>(() => {
    if (!topicDetails?.views) return [];
    return topicDetails.views.map((view) => ({
      viewName: view.view_name,
      name: view.name,
      description: view.description,
      datasource: view.datasource || "",
      table: view.table || "",
      dimensions: view.dimensions || [],
      measures: view.measures || [],
    }));
  }, [topicDetails]);

  const availableDimensions = useMemo(() => {
    return viewsWithData.flatMap((view) =>
      view.dimensions.map((dim) => ({
        name: `${view.viewName}.${dim.name}`,
        fullName: `${view.viewName}.${dim.name}`,
      })),
    );
  }, [viewsWithData]);

  const availableMeasures = useMemo(() => {
    return viewsWithData.flatMap((view) =>
      view.measures.map((measure) => ({
        name: `${view.viewName}.${measure.name}`,
        fullName: `${view.viewName}.${measure.name}`,
      })),
    );
  }, [viewsWithData]);

  const canExecuteQuery = useMemo(() => {
    return viewsWithData.length > 0;
  }, [viewsWithData]);

  const topicData = useMemo<TopicData | null>(() => {
    if (!topicDetails?.topic) return null;
    return {
      name: topicDetails.topic.name,
      description: topicDetails.topic.description,
      views: topicDetails.topic.views || [],
      base_view: topicDetails.topic.base_view,
    };
  }, [topicDetails]);

  const topicContextValue = useMemo<TopicExplorerContextType>(
    () => ({
      topicData,
      viewsWithData,
      topicLoading,
      loadingTopicError: loadingTopicError?.message,
      refetchTopicDetails,
    }),
    [
      topicData,
      viewsWithData,
      topicLoading,
      loadingTopicError,
      refetchTopicDetails,
    ],
  );

  return (
    <TopicExplorerContext.Provider value={topicContextValue}>
      <SemanticExplorerProvider
        topic={topicData?.name}
        dataLoading={topicLoading}
        loadingError={loadingTopicError?.message}
        availableDimensions={availableDimensions}
        availableMeasures={availableMeasures}
        canExecuteQuery={canExecuteQuery}
      >
        {children}
      </SemanticExplorerProvider>
    </TopicExplorerContext.Provider>
  );
};

export const TopicExplorerProvider = ({
  children,
}: TopicExplorerProviderProps) => {
  return <TopicExplorerProviderInner>{children}</TopicExplorerProviderInner>;
};

export const useTopicExplorerContext = () => {
  const semanticContext = useSemanticExplorerContext();
  const topicContext = useContext(TopicExplorerContext);

  if (!topicContext) {
    throw new Error(
      "useTopicExplorerContext must be used within TopicExplorerProvider",
    );
  }

  return {
    ...semanticContext,
    ...topicContext,
  };
};
