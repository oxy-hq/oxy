use crate::config::model::LookerQueryParams;

pub struct LookerQueryInput {
    pub params: LookerQueryParams,
    pub explore: String,
    pub model: String,
    pub integration: String,
}
