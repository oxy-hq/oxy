import base64
import cgi
import json
from datetime import datetime
from email.feedparser import FeedParser
from typing import Any, cast

import orjson
from aiohttp import MultipartReader, MultipartWriter, hdrs
from multidict import istr
from onyx.catalog.ingest.adapters.utils import EmailRecord, eml_for_record, parse_email
from onyx.catalog.ingest.base.auth import OAuthenticator
from onyx.catalog.ingest.base.context import StreamContext
from onyx.catalog.ingest.base.processor import ProcessingStrategy
from onyx.catalog.ingest.base.rest import RESTSource, RESTStream
from onyx.catalog.ingest.base.types import OAuthConfig, Record
from onyx.shared.logging import Logged
from tenacity import AsyncRetrying, stop_after_attempt, wait_exponential_jitter
from typing_extensions import TypedDict


class GmailHeader(TypedDict):
    name: str
    value: str


class GmailPartBody(TypedDict):
    data: str


class GmailPart(TypedDict):
    body: GmailPartBody
    headers: list[GmailHeader]
    mimeType: str


class GmailPayload(TypedDict):
    headers: list[GmailHeader]
    parts: list[GmailPart]


class GmailRecord(Record):
    id: str
    thread_id: str
    label_ids: list[str]
    snippet: str
    history_id: str
    payload: str
    internal_date: datetime


class GmailOAuthConfig(OAuthConfig):
    endpoint: str = "https://oauth2.googleapis.com/token"


class GmailResponse(TypedDict):
    messages: list[dict] | None
    nextPageToken: str


class GmailState(TypedDict):
    start_date: int


class GmailCursor(TypedDict):
    pageToken: str
    q: str
    maxResults: int


class GmailStream(
    Logged,
    RESTStream[GmailResponse],
):
    def stream_context(self, context, config, encoder, storage) -> StreamContext:
        return StreamContext(
            name="messages",
            ingest_context=context,
            properties=[
                ("id", "!string"),
                ("thread_id", "!string"),
                ("label_ids", "!array<!string>"),
                ("snippet", "!string"),
                ("history_id", "!string"),
                ("internal_date", "!timestamp"),
                ("payload", "!string"),
            ],
            key_properties=["id"],
            bookmark_property="internal_date",
            embedding_strategy=GmailProcessingStrategy(
                config=config,
                encoder=encoder,
                stream_name=str(context.identity.slug),
            ),
            state_storage=storage,
        )

    async def _extract_records(self, response):
        message_ids = [message["id"] for message in response.get("messages", [])]  # type: ignore
        self.log.info(f"Extracting {len(message_ids)} records")
        results = {}

        async for attempt in AsyncRetrying(
            stop=stop_after_attempt(5),
            wait=wait_exponential_jitter(),
        ):
            with attempt:
                if attempt.retry_state.attempt_number > 1:
                    self.log.info(f"Retrying batch messages: {len(message_ids)}")

                messages, failed_ids = await self.__batch_messages(message_ids)
                results.update(messages)
                if failed_ids:
                    message_ids = failed_ids
                    raise RuntimeError(f"Failed to batch messages: {failed_ids}")

        records: list[GmailRecord] = []
        for result in results.values():
            record = GmailRecord.model_validate(result)
            records.append(record)

        return records

    def _extract_cursor(self, response):
        cursor = response.get("nextPageToken")
        self.log.info(f"Extracting cursor: {cursor}")
        return cursor

    def _merge_cursor(self, request, cursor: Any | None) -> dict:
        if not cursor:
            return request
        params = {
            **request.get("params", {}),
            "pageToken": cursor,
        }
        return {
            **request,
            "params": params,
        }

    def _request_factory(self, context):
        default_request = {
            "url": "/gmail/v1/users/me/messages",
            "method": "GET",
        }

        # extract start date from state
        after = context.request_interval.start
        before = context.request_interval.end
        q = f"after:{after} before:{before}"

        return {
            **default_request,
            "params": {"q": q, "maxResults": context.batch_size},
        }

    def __deserialize_part(self, part_response: str):
        # Strip off the status line
        status_line, part_response = part_response.split("\n", 1)
        protocol, status, reason = status_line.split(" ", 2)

        # Parse the rest of the response
        parser = FeedParser()
        parser.feed(part_response)
        msg = parser.close()
        msg["status"] = status
        content = part_response.split("\r\n\r\n", 1)[1]
        self.log.info(f"Deserialized part: {protocol} {status} {reason}")
        message = orjson.loads(content)

        if "payload" in message:
            message["payload"] = orjson.dumps(message["payload"])

        if "internalDate" in message:
            message["internalDate"] = datetime.fromtimestamp(int(message["internalDate"]) / 1000)

        return message, int(status)

    async def __batch_messages(self, message_ids: list[str]):
        self.log.info(f"Batching messages: {len(message_ids)}")
        batch_path = "/batch/gmail/v1"
        messages = {}
        failed_ids = []

        if not message_ids:
            return messages, failed_ids

        with MultipartWriter("mixed", boundary="batch_gmail_read") as mpwriter:
            for message_id in message_ids:
                mpwriter.append(
                    f"GET /gmail/v1/users/me/messages/{message_id}",
                    {"CONTENT-TYPE": "application/http", "CONTENT-ID": message_id},  # type: ignore
                )

            async with self.session.post(batch_path, data=mpwriter) as response:
                response.raise_for_status()
                mpreader = MultipartReader.from_response(response)
                self.log.info(f"Reading batch response: {response}")
                idx = 0

                while True:
                    part = await mpreader.next()
                    if part is None:
                        break

                    if part.headers[hdrs.CONTENT_TYPE] == "application/http":
                        message_id = part.headers[istr("Content-Id")].split("response-")[1]
                        part_response = await part.text()  # type: ignore
                        message, status = self.__deserialize_part(part_response)
                        if status == 200:
                            messages[message_id] = message
                        else:
                            failed_ids.append(message_id)
                        continue

                    idx += 1
        return messages, failed_ids


class GmailSource(RESTSource):
    base_url = "https://gmail.googleapis.com"
    authenticator = OAuthenticator()

    def streams(self, session):
        return [GmailStream(session)]


class GmailProcessingStrategy(ProcessingStrategy[GmailRecord]):
    def __find_part(self, payload, mime_type):
        if payload.get("mimeType") == mime_type:
            return payload

        if "parts" in payload:
            for p in payload["parts"]:
                found = self.__find_part(p, mime_type)
                if found:
                    return found

    def __find_headers(self, headers, name, default_value=None):
        for h in headers:
            if h["name"] == name:
                return h["value"]
        return default_value

    def __decode_part(self, part: GmailPart):
        content_type = ""
        part_headers = part.get("headers", [])
        content_type = self.__find_headers(part_headers, "Content-Type", "")
        _, params = cgi.parse_header(cast(str, content_type))
        charset = params.get("charset", "utf-8")
        encoded_body = part.get("body", {}).get("data", "")
        body = base64.urlsafe_b64decode(encoded_body).decode(charset, errors="ignore")
        return body

    def __extract_payload(self, record: GmailRecord):
        payload: GmailPayload = json.loads(record.payload)
        return payload

    def __extract_subject(self, payload: GmailPayload):
        headers = payload.get("headers", [])
        return self.__find_headers(headers, "Subject", None)

    def __extract_to_email(self, payload: GmailPayload):
        headers = payload.get("headers", [])
        return self.__find_headers(headers, "Delivered-To", None)

    def __extract_email(self, record: GmailRecord):
        payload = self.__extract_payload(record)
        headers = payload.get("headers", [])
        subject = self.__extract_subject(payload)
        from_email = self.__find_headers(headers, "From", None)
        to_email = self.__extract_to_email(payload)
        date = self.__find_headers(headers, "Date", None)
        message_id = self.__find_headers(headers, "Message-Id", None)

        plaintext_part = self.__find_part(payload, "text/plain")
        html_part = self.__find_part(payload, "text/html")
        plaintext_body = record.snippet
        html_body = ""

        if plaintext_part:
            plaintext_body = self.__decode_part(plaintext_part)

        if html_part:
            html_body = self.__decode_part(html_part)

        return EmailRecord(
            html_body=html_body,
            text_body=plaintext_body,
            from_email=from_email,
            to_email=to_email,
            subject=subject,
            date=date,
            message_identifier=message_id,
        )

    def _build_doc_id(self, record: GmailRecord) -> str:
        return record.id

    def _build_timestamp(self, record: GmailRecord) -> int:
        return int(record.internal_date.timestamp())

    def _build_doc(self, record: GmailRecord) -> str:
        return parse_email(eml_for_record(self.__extract_email(record)))

    def _build_metadata(self, record: GmailRecord) -> list[str]:
        metadata = super()._build_metadata(record)
        email = self.__extract_email(record)
        from_email = email.from_email
        to_email = email.to_email
        subject = email.subject
        return [
            *metadata,
            f"from_email==={from_email}",
            f"to_email==={to_email}",
            f"mail_subject==={subject}",
        ]

    def _build_doc_url(self, record: GmailRecord) -> str:
        payload = self.__extract_payload(record)
        to_email = self.__extract_to_email(payload)
        return f"https://mail.google.com/mail/u/{to_email}/#inbox/{record.id}"

    def _build_doc_title(self, record: GmailRecord) -> str:
        payload = self.__extract_payload(record)
        subject = self.__extract_subject(payload)
        return subject or ""
