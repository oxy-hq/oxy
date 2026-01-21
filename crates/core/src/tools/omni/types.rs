use crate::types::tool_params::OmniQueryParams;
use serde::Serialize;

#[derive(Serialize)]
pub struct OmniQueryInput {
    pub params: OmniQueryParams,
    pub topic: String,
    pub integration: String,
}
