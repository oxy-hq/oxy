pub mod types;

use std::{fs::File, path::PathBuf};

use crate::{
    constants::UNPUBLISH_APP_DIR,
    errors::OxyError,
    execute::{
        Executable, ExecutionContext,
        types::{Output, event::DataApp},
    },
    service,
    tools::types::CreateDataAppInput,
    utils::find_project_path,
};
use short_uuid::ShortUuid;
use tokio::fs;

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
        tracing::debug!("Creating data app with input: {:?}", &input);
        let CreateDataAppInput { param } = input;
        let project_path = find_project_path()?;
        let mut full_file_name = format!("{}.app.yml", param.file_name);
        let file_dir = project_path.join(UNPUBLISH_APP_DIR);
        if !file_dir.exists() {
            fs::create_dir_all(&file_dir).await?;
        }
        let mut file_path = file_dir.join(&full_file_name);
        // check if the file already exists
        if file_path.exists() {
            full_file_name = format!("{}_{}.app.yml", param.file_name, ShortUuid::generate());
            file_path.set_file_name(&full_file_name);
        }
        println!("Creating data app at: {}", file_path.display());
        let mut file = File::create(&file_path).map_err(|e| anyhow::anyhow!(e))?;
        let config = param.app_config;
        // write config to file
        serde_yml::to_writer(&mut file, &config).map_err(|e| anyhow::anyhow!(e))?;
        println!("Data app created at: {}", file_path.display());
        let file_relative_path = PathBuf::from(UNPUBLISH_APP_DIR).join(&full_file_name);
        service::app::clean_up_app_data(&file_relative_path).await?;
        println!("Data app cleaned up at: {}", file_path.display());
        execution_context
            .write_data_app(DataApp {
                file_path: file_relative_path.clone(),
            })
            .await?;
        return Ok(Output::Text(format!(
            "Data app created at: {}",
            file_relative_path.display()
        )));
    }
}
