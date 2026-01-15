use std::{fs::File, path::PathBuf};

use crate::{
    config::model::AppConfig,
    execute::{
        Executable, ExecutionContext,
        types::{Output, event::DataApp},
    },
};
use oxy_shared::errors::OxyError;
use short_uuid::ShortUuid;

#[derive(Debug, Clone)]
pub struct CreateDataAppInput {
    pub param: CreateDataAppParams,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct CreateDataAppParams {
    pub file_name: String,
    pub app_config: AppConfig,
}

#[derive(Debug, Clone)]
pub struct CreateDataAppExecutable;

#[async_trait::async_trait]
impl Executable<CreateDataAppInput> for CreateDataAppExecutable {
    type Response = Output;

    async fn execute(
        &mut self,
        execution_context: &ExecutionContext,
        input: CreateDataAppInput,
    ) -> Result<Self::Response, OxyError> {
        log::debug!("Creating data app with input: {:?}", &input);
        let CreateDataAppInput { param } = input;
        let project_path = execution_context.project.config_manager.project_path();
        let mut full_file_name = format!("{}.app.yml", param.file_name);
        let mut file_path = project_path.join(&full_file_name);

        // check if the file already exists
        if file_path.exists() {
            full_file_name = format!("{}_{}.app.yml", param.file_name, ShortUuid::generate());
            file_path = project_path.join(&full_file_name);
        }

        log::info!("Creating data app at: {}", file_path.display());
        let mut file = File::create(&file_path)
            .map_err(|e| OxyError::RuntimeError(format!("Failed to create file: {}", e)))?;
        let config = param.app_config;

        // write config to file
        serde_yaml::to_writer(&mut file, &config)
            .map_err(|e| OxyError::RuntimeError(format!("Failed to write YAML: {}", e)))?;

        log::info!("Data app created at: {}", file_path.display());

        execution_context
            .write_data_app(DataApp {
                file_path: PathBuf::from(full_file_name.clone()),
            })
            .await?;

        Ok(Output::Text(format!(
            "Data app created at: {}",
            full_file_name
        )))
    }
}
