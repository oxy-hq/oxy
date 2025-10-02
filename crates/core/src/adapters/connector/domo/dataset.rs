use std::{collections::HashMap, error::Error};

use crate::{
    adapters::connector::{
        DOMO,
        domo::types::{DataDictionaryColumn, DatasetDetails, DatasetInfo},
    },
    errors::OxyError,
};

#[derive(Debug)]
pub struct DOMODataset<'a> {
    domo: &'a DOMO,
    base_path: String,
}

impl<'a> DOMODataset<'a> {
    pub fn new(domo: &'a DOMO) -> Self {
        DOMODataset {
            domo,
            base_path: "query/v1/datasources".to_string(),
        }
    }

    async fn info(&self, dataset_id: &str) -> Result<DatasetInfo, OxyError> {
        let info_url = format!("/data/v3/datasources/{dataset_id}");
        let response =
            self.domo.get(&info_url).send().await.map_err(|err| {
                OxyError::RuntimeError(format!("Failed to send request: {}", err))
            })?;
        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(OxyError::RuntimeError(format!(
                "DOMO API request failed with status {}: {}",
                status, text
            )));
        }
        let info = response.json().await.map_err(|err| {
            OxyError::RuntimeError(format!("Failed to parse json response: {:?}", err.source()))
        })?;
        Ok(info)
    }

    async fn dictionary(
        &self,
        dataset_id: &str,
    ) -> Result<HashMap<String, DataDictionaryColumn>, OxyError> {
        let ai = self.domo.ai();
        if let Some(dict) = ai.data_dictionary(dataset_id).await? {
            let mapping = dict
                .columns
                .into_iter()
                .map(|col| (col.name.clone(), col))
                .collect();
            Ok(mapping)
        } else {
            Ok(HashMap::new())
        }
    }

    pub async fn details(&self, dataset_id: &str) -> Result<DatasetDetails, OxyError> {
        let url = format!("/{}/{dataset_id}/schema/indexed", self.base_path);
        let response =
            self.domo.get(&url).send().await.map_err(|err| {
                OxyError::RuntimeError(format!("Failed to send request: {}", err))
            })?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(OxyError::RuntimeError(format!(
                "DOMO API request failed with status {}: {}",
                status, text
            )));
        }
        let mut json: DatasetDetails = response.json().await.map_err(|err| {
            OxyError::RuntimeError(format!("Failed to parse json response: {:?}", err.source()))
        })?;
        let info = self.info(dataset_id).await?;
        json.description = format!("{}\n{}", info.name, info.description);
        let dict = self.dictionary(dataset_id).await?;
        json.tables.iter_mut().for_each(|table| {
            table.columns.iter_mut().for_each(|col| {
                if let Some(ai_col) = dict.get(&col.name) {
                    col.description = Some(ai_col.description.clone());
                    col.synonyms = Some(ai_col.synonyms.clone());
                }
            });
        });

        Ok(json)
    }
}
