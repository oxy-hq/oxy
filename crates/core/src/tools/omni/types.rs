use crate::types::tool_params::OmniQueryParams;

pub struct OmniQueryInput {
    pub params: OmniQueryParams,
    pub topic: String,
    pub integration: String,
}
