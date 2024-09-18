from dataclasses import dataclass
from typing import Sequence

from langchain.chains.query_constructor.base import AttributeInfo


@dataclass
class MetadataSchema:
    fields: Sequence[AttributeInfo]
    content_description: str


filtering_metadata_schema = MetadataSchema(
    fields=[
        AttributeInfo(
            name="channel_name",
            description="Name of the Slack channel",
            type="string",
        ),
        AttributeInfo(
            name="channel_creator_name",
            description="Name of the creator of the Slack channel",
            type="string",
        ),
        AttributeInfo(
            name="message_author",
            description="Author of the message in Slack",
            type="string",
        ),
        AttributeInfo(
            name="timestamp",
            description="Date the document was created, in 'YYYY-MM-DD' format",
            type="string",
        ),
        AttributeInfo(
            name="from_email",
            description="Email sender's name in Gmail",
            type="string",
        ),
        AttributeInfo(
            name="to_email",
            description="Email recipient's name in Gmail",
            type="string",
        ),
        AttributeInfo(
            name="mail_subject",
            description="Subject of the email in Gmail",
            type="string",
        ),
        AttributeInfo(
            name="notion_page_title",
            description="Title of the Notion page",
            type="string",
        ),
        AttributeInfo(
            name="notion_page_creator",
            description="Creator's name of the Notion page",
            type="string",
        ),
        AttributeInfo(
            name="salesforce_reseller_name",
            description="Name of the reseller in Salesforce",
            type="string",
        ),
        AttributeInfo(
            name="salesforce_doc_title",
            description="Title of the document in Salesforce",
            type="string",
        ),
        AttributeInfo(
            name="source_type",
            description="Type of source document: Slack, Notion, Gmail, Salesforce",
            type="string",
        ),
    ],
    content_description="Description of a document from a source",
)
