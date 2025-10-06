use crate::tools::types::OmniQueryParams;

pub struct OmniQueryInput {
    pub params: OmniQueryParams,
    pub topic: String,
    pub integration: String,
}

#[derive(Clone)]
pub struct OmniQueryToolInput {
    pub param: String,
    pub topic: String,
    pub integration: String,
}
