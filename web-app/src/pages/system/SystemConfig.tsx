import React from "react";

import { css } from "styled-system/css";

interface Warehouse {
  name: string;
  type: string;
  key_path: string;
  dataset: string;
}

interface Model {
  name: string;
  vendor: string;
  key_var: string;
  model_ref: string;
}

interface Defaults {
  agent: string;
  project_path: string;
}

interface Retrieval {
  name: string;
  embed_model: string;
  rerank_model: string;
  top_k: number;
  factor: number;
}

export interface SystemData {
  warehouses: Warehouse[];
  models: Model[];
  defaults: Defaults;
  retrievals: Retrieval[];
}

interface SystemPageProps {
  data: SystemData;
}

const SystemPage: React.FC<SystemPageProps> = ({ data }) => {
  const containerStyle = css({
    maxW: "4xl",
    mx: "auto",
    zIndex: "2"
  });

  const cardStyle = css({
    mb: "4",
    p: "6",
    border: "2px solid token(colors.primary)",
    borderRadius: "md",
    bg: "token(colors.background)",
    display: "flex",
    flexDirection: "column",
    gap: "3"
  });

  const headerStyle = css({
    pb: "2",
    mb: "2",
    borderBottom: "1px solid token(colors.primary)",
    fontSize: "sm"
  });

  const sectionStyle = css({
    fontSize: "sm",
    border: "1px solid token(colors.primary)",
    borderRadius: "md",
    p: "3"
  });

  const titleStyle = css({ color: "token(colors.primary)" });

  const listItemStyle = css({ color: "token(colors.lightGray)" });

  const modelItemStyle = css({
    color: "token(colors.lightGray)",
    borderBottom: "1px solid token(colors.primary)",
    pb: "2",
    mb: "2"
  });

  const footerStyle = css({
    textAlign: "center",
    fontSize: "xs",
    color: "rgba(0, 255, 0, 0.7)"
  });

  return (
    <div className={containerStyle}>
      <div className={cardStyle}>
        <div className={headerStyle}>
          <div>SYSTEM CONFIG</div>
        </div>

        <div className={sectionStyle}>
          <h2 className={titleStyle}>Warehouses</h2>
          <ul>
            {data.warehouses.map((warehouse, index) => (
              <li key={index + warehouse.key_path} className={listItemStyle}>
                <strong>Name:</strong> {warehouse.name} <br />
                <strong>Type:</strong> {warehouse.type} <br />
                <strong>Key Path:</strong> {warehouse.key_path} <br />
                <strong>Dataset:</strong> {warehouse.dataset}
              </li>
            ))}
          </ul>
        </div>

        <div className={sectionStyle}>
          <h2 className={titleStyle}>Models</h2>
          <ul>
            {data.models.map((model, index) => (
              <li key={index + model.key_var} className={modelItemStyle}>
                <strong>Name:</strong> {model.name} <br />
                <strong>Vendor:</strong> {model.vendor} <br />
                <strong>Key Variable:</strong> {model.key_var} <br />
                <strong>Model Reference:</strong> {model.model_ref}
              </li>
            ))}
          </ul>
        </div>

        <div className={sectionStyle}>
          <h2 className={titleStyle}>Retrievals</h2>
          <ul>
            {data.retrievals.map((retrieval, index) => (
              <li key={index + retrieval.name} className={listItemStyle}>
                <strong>Name:</strong> {retrieval.name} <br />
                <strong>Embed Model:</strong> {retrieval.embed_model} <br />
                <strong>Rerank Model:</strong> {retrieval.rerank_model} <br />
                <strong>Top K:</strong> {retrieval.top_k} <br />
                <strong>Factor:</strong> {retrieval.factor}
              </li>
            ))}
          </ul>
        </div>

        <div className={sectionStyle}>
          <h2 className={titleStyle}>Defaults</h2>
          <p className={listItemStyle}>
            <strong>Agent:</strong> {data.defaults.agent} <br />
            <strong>Project Path:</strong> {data.defaults.project_path}
          </p>
        </div>
      </div>
      <div className={footerStyle}>PROPERTY OF ONYX SYSTEMS INC.</div>
    </div>
  );
};

export default SystemPage;

