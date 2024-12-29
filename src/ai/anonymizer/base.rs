use std::collections::HashMap;

use crate::errors::OnyxError;

pub trait Anonymizer: AnonymizerClone {
    fn anonymize(
        &self,
        text: &str,
        items: Option<HashMap<String, String>>,
    ) -> Result<(String, HashMap<String, String>), OnyxError>;

    fn deanonymize(&self, text: &str, items: &HashMap<String, String>) -> String {
        let mut result = text.to_string();
        for (keyword, replacement) in items {
            result = result.replace(keyword, replacement);
        }
        log::info!(
            "[DeAnonymizer] Replaced:\n---{}\n---with:---\n{}",
            text,
            result
        );
        result
    }
}

pub trait AnonymizerClone {
    fn clone_box(&self) -> Box<dyn Anonymizer>;
}

impl<T> AnonymizerClone for T
where
    T: 'static + Anonymizer + Clone,
{
    fn clone_box(&self) -> Box<dyn Anonymizer> {
        Box::new(self.clone())
    }
}

impl Clone for Box<dyn Anonymizer> {
    fn clone(&self) -> Box<dyn Anonymizer> {
        self.clone_box()
    }
}
