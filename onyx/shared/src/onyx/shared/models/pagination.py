from pydantic import BaseModel, Field

MAX_PAGE_SIZE = 100
DEFAULT_PAGE_SIZE = 10
DEFAULT_PAGE = 1


class PaginationMetadata:
    page = 1
    page_size = 0
    total_count = 0

    def __init__(self, page, page_size, total_count) -> None:
        self.page = page
        self.page_size = page_size
        self.total_count = total_count

    def to_dict(self):
        return {
            "page": self.page,
            "page_size": self.page_size,
            "total_count": self.total_count,
        }


class PaginationParams(BaseModel):
    page: int = Field(ge=1, default=DEFAULT_PAGE)
    page_size: int = Field(ge=1, le=MAX_PAGE_SIZE, default=DEFAULT_PAGE_SIZE)

    @property
    def offset(self):
        return (self.page - 1) * self.page_size

    def paginate_query(self, query):
        paginated_query = query.limit(self.page_size).offset((self.page - 1) * self.page_size)
        pagination_metadata = PaginationMetadata(page=self.page, page_size=self.page_size, total_count=query.count())
        return paginated_query, pagination_metadata
