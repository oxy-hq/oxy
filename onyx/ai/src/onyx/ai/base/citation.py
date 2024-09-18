import re

from langchain_core.documents import Document
from onyx.shared.logging import Logged
from onyx.shared.models.common import Source

CitationSource = Document


class CitationMarker(Logged):
    uncompleted_source_mark_pattern = r":s?\[?(\d+)?$"
    source_mark_regex = r":s\[(\d+)\]"

    def __init__(self) -> None:
        self.counter = 0
        self.mapping: dict[int, CitationSource] = {}
        self.used_markers: dict[int, int] = {}

    def reset(self):
        self.counter = 0
        self.mapping = {}
        self.used_markers = {}

    def get_citation(self, source: CitationSource) -> str:
        self.counter += 1
        self.mapping[self.counter] = source
        return f":s[{self.counter}]"

    def map_source(self, content: str) -> str:
        marked_content = content
        for origin, replacement in self.used_markers.items():
            marked_content = marked_content.replace(f":s[{origin}]", f":s[{replacement}]")
        return marked_content

    def _add_marker_if_not_exists(self, source_number: int) -> int:
        if source_number not in self.used_markers:
            # source start from 1
            # and increase every time a new source is added
            self.used_markers[source_number] = len(self.used_markers) + 1
        return self.used_markers[source_number]

    def _citation_to_source(self, source_number: int) -> Source | None:
        document = self.mapping.get(source_number)
        used_source_number = self.used_markers.get(source_number)

        if isinstance(document, Document) and used_source_number is not None:
            source_type = document.metadata.get("source_type", "unknown")
            url = document.metadata.get("url", "")
            page = document.metadata.get("page", "")
            label = document.metadata.get("title", source_type)
            return Source(
                number=used_source_number,
                label=label,
                content=document.page_content,
                type=source_type,
                url=url,
                page=page,
            )
        else:
            return None

    def _replace_str_index(self, text: str, start: int, end: int, replacement: str):
        return f"{text[:start]}{replacement}{text[end:]}"

    def mark_used(self, content: str) -> tuple[str, list[Source]]:
        marked_content = content
        sources: list[Source] = []
        source_numbers = set()
        # for each source mark, re-number it to ensure order
        for match in re.finditer(self.source_mark_regex, content):
            real_source_number = int(match.groups()[0])
            used_source_number = self._add_marker_if_not_exists(real_source_number)
            # replace the source mark with the new number
            start, end = match.span()
            marked_content = self._replace_str_index(content, start, end, f":s[{used_source_number}]")
            source_numbers.add(real_source_number)

        for source_number in source_numbers:
            source = self._citation_to_source(source_number)
            if source:
                sources.append(source)

        return marked_content, sources


class CitationState(Logged):
    valid_markers = [":", "s", "["]
    terminated_mark = "]"

    def __init__(self) -> None:
        self.__state = 0
        self.__buffer = ""
        self.__source_started = False
        self.__source_number = ""

    def is_empty(self) -> bool:
        return self.__state == 0 and not self.__buffer

    def reset(self):
        self.__state = 0
        self.__buffer = ""
        self.__source_started = False
        self.__source_number = ""

    def __is_valid_number(self, source: str) -> bool:
        try:
            return int(source) >= 0
        except ValueError:
            return False

    def __process_char(self, char: str) -> str | None:
        allowed_char = self.valid_markers[self.__state]

        if len(self.valid_markers) == self.__state + 1:
            self.__buffer += char

            if char == self.terminated_mark:
                try:
                    if not self.__is_valid_number(self.__source_number):
                        self.log.warning(f"Invalid source number: {self.__source_number}")
                        return ""

                    self.log.debug(f"Yield source number: {self.__source_number}")
                    return self.__buffer
                finally:
                    self.reset()

            # Beginning of the source number
            if self.__source_started:
                self.__source_number += char

            if char == self.valid_markers[-1] and not self.__source_started:
                self.__source_started = True

            return None

        if char != allowed_char:
            try:
                return self.__buffer + char
            finally:
                self.reset()

        self.__state += 1
        self.__buffer += char

    def process(self, char: str) -> str | None:
        return self.__process_char(char)
