import email
from dataclasses import dataclass
from datetime import datetime
from email.message import Message
from email.utils import formatdate
from string import Template
from textwrap import dedent
from typing import cast

from bs4 import BeautifulSoup
from dateutil import parser
from onyx.catalog.ingest.base.utils import clean_ascii_control_chars, clean_non_ascii_chars
from onyx.shared.logging import get_logger

logger = get_logger(__name__)


EMAIL_TEMPLATE = Template(
    """MIME-Version: 1.0
Date: $date
Message-ID: $message_identifier
Subject: $subject
From: $from_email
To: $to_email
Content-Type: multipart/alternative; boundary="00000000000095c9b205eff92630"
--00000000000095c9b205eff92630
Content-Type: text/plain; charset="UTF-8"
$text_body
--00000000000095c9b205eff92630
Content-Type: text/html; charset="UTF-8"
$html_body
--00000000000095c9b205eff92630--
""",
)


@dataclass
class EmailRecord:
    date: str | None = None
    message_identifier: str | None = None
    subject: str | None = None
    from_email: str | None = None
    to_email: str | None = None
    text_body: str | None = None
    html_body: str = ""


def eml_for_record(record: EmailRecord) -> str:
    parsed_date = None

    try:
        if record.date:
            parsed_date = formatdate(cast(datetime, parser.parse(record.date)).timestamp())
    except Exception:
        pass

    eml = EMAIL_TEMPLATE.substitute(
        date=parsed_date,
        message_identifier=record.message_identifier,
        subject=record.subject,
        from_email=record.from_email,
        to_email=record.to_email,
        text_body=record.text_body,
        html_body=record.html_body.replace("<br />", "<p>").replace("<body", "<body><p"),
    )
    return dedent(eml)


def parse_email(raw_msg: str):
    mime_msg = email.message_from_string(raw_msg)
    body = None
    # If the message body contains HTML, parse it with BeautifulSoup
    if "text/html" in mime_msg:
        body = __extract_email_html(raw_msg)

    if not body:
        body = __extract_email_plain_text(mime_msg)
    return clean_ascii_control_chars(clean_non_ascii_chars(body))


def __extract_email_html(raw_msg: str):
    try:
        soup = BeautifulSoup(raw_msg, "html.parser")
        extracted = soup.get_text()
        return extracted
    except Exception as e:
        logger.warning(f"Error extracting message body: {e}", exc_info=True)
        return None


def __extract_email_plain_text(mime_msg: Message):
    body_text = ""
    if mime_msg.get_content_type() == "text/plain":
        plain_text = mime_msg.get_payload(decode=True)
        body_text = cast(bytes, plain_text).decode("ascii", errors="ignore").encode("utf-8").decode(errors="ignore")

    elif mime_msg.get_content_maintype() == "multipart":
        msg_parts = mime_msg.get_payload()
        for msg_part in msg_parts:
            body_text += __extract_email_plain_text(cast(Message, msg_part))

    return body_text
