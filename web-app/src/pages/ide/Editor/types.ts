export interface Dimension {
  name: string;
  type: string;
  description?: string;
  expr: string;
}

export interface Measure {
  name: string;
  type: string;
  description?: string;
  expr?: string;
}

export interface ViewData {
  name: string;
  description?: string;
  datasource: string;
  table: string;
  dimensions: Dimension[];
  measures: Measure[];
}

export interface TopicData {
  name: string;
  description?: string;
  views: string[];
  base_view?: string;
}

export interface ViewWithData extends ViewData {
  viewName: string;
}
