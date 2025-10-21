#[derive(serde::Serialize, Debug)]
pub struct ExecuteQueryRequest {
    pub sql: String,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ColumnMetadata {
    pub r#type: String,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecuteQueryResponse {
    pub metadata: Vec<ColumnMetadata>,
    pub columns: Vec<String>,
    pub rows: Vec<Vec<serde_json::Value>>,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DatasetSchemaField {
    pub name: String,
    pub r#type: String,
    #[serde(skip)]
    pub description: Option<String>,
    #[serde(skip)]
    pub synonyms: Option<Vec<String>>,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DatasetSchema {
    pub columns: Vec<DatasetSchemaField>,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DatasetDetails {
    pub name: String,
    #[serde(skip)]
    pub description: String,
    pub tables: Vec<DatasetSchema>,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DatasetInfo {
    pub name: String,
    pub description: String,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DataDictionaryColumn {
    pub name: String,
    pub description: String,
    pub synonyms: Vec<String>,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DataDictionaryInfo {
    pub columns: Vec<DataDictionaryColumn>,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DataDictionaryWrapper {
    pub data_dictionary: DataDictionaryInfo,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DataDictionaryResponse(pub Vec<DataDictionaryWrapper>);
