use std::error::Error;

use crate::connector::{
    DOMO,
    domo::types::{DataDictionaryInfo, DataDictionaryResponse},
};
use oxy_shared::errors::OxyError;

#[derive(Debug)]
pub struct DOMOAI<'a> {
    domo: &'a DOMO,
    base_path: String,
}

impl<'a> DOMOAI<'a> {
    pub fn new(domo: &'a DOMO) -> Self {
        DOMOAI {
            domo,
            base_path: "/ai/readiness/v1".to_string(),
        }
    }

    pub async fn data_dictionary(
        &self,
        dataset_id: &str,
    ) -> Result<Option<DataDictionaryInfo>, OxyError> {
        let url = format!(
            "{}{}/data-dictionary/dataset/{dataset_id}",
            self.domo.base_url, self.base_path
        );
        let response =
            self.domo.client.get(&url).send().await.map_err(|err| {
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
        let json: DataDictionaryResponse = response.json().await.map_err(|err| {
            OxyError::RuntimeError(format!("Failed to parse json response: {:?}", err.source()))
        })?;
        let result = json
            .0
            .into_iter()
            .next()
            .map(|wrapper| wrapper.data_dictionary);
        Ok(result)
    }
}
