use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Serialize, Deserialize, Debug, Clone, ToSchema)]
pub struct Pagination {
    pub size: usize,
    pub page: usize,
    pub num_pages: Option<usize>,
}

#[derive(Serialize, Deserialize, Debug, Clone, ToSchema)]
pub struct Paginated<T> {
    pub items: Vec<T>,
    pub pagination: Pagination,
}
