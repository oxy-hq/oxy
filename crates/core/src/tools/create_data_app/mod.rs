pub mod types;

use std::{fs::File, path::PathBuf};

use crate::{
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
        let mut file_path = project_path.join(&full_file_name);
        // check if the file already exists
        if file_path.exists() {
            full_file_name = format!("{}_{}.app.yml", param.file_name, ShortUuid::generate());
            file_path.set_file_name(&full_file_name);
        }
        println!("Creating data app at: {}", file_path.display());
        let mut file = File::create(&file_path).map_err(|e| anyhow::anyhow!(e))?;
        let config = param.app_config;
        // write config to file
        serde_yaml::to_writer(&mut file, &config).map_err(|e| anyhow::anyhow!(e))?;
        println!("Data app created at: {}", file_path.display());
        service::app::clean_up_app_data(&PathBuf::from(&full_file_name)).await?;
        println!("Data app cleaned up at: {}", file_path.display());
        execution_context
            .write_data_app(DataApp {
                file_path: PathBuf::from(full_file_name.clone()),
            })
            .await?;
        return Ok(Output::Text(format!(
            "Data app created at: {}",
            full_file_name
        )));
    }
}
