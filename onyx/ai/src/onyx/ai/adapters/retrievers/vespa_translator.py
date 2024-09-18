import re
from datetime import datetime, timedelta
from typing import Dict, Tuple, Union

from langchain.chains.query_constructor.ir import (
    Comparator,
    Comparison,
    Operation,
    Operator,
    StructuredQuery,
    Visitor,
)
from onyx.shared.logging import Logged


def replace_non_alphanumeric_with_wildcards(input_string):
    transformed_string = re.sub(r"[^a-zA-Z0-9]+", ".*", input_string)
    return ".*" + transformed_string + ".*"


class VespaTranslator(Logged, Visitor):
    """Translate `Vespa` internal query language elements to valid filters."""

    allowed_comparators = (
        Comparator.EQ,
        Comparator.LT,
        Comparator.GT,
        Comparator.GTE,
        Comparator.LTE,
        Comparator.CONTAIN,
    )

    """Subset of allowed logical comparators."""
    allowed_operators = (Operator.AND, Operator.OR)
    """Subset of allowed logical operators."""

    map_dict = {
        Comparator.EQ: " = ",
        Comparator.GT: " > ",
        Comparator.GTE: " >= ",
        Comparator.LT: " < ",
        Comparator.LTE: " <= ",
    }

    def _format_func(self, func: Union[Operator, Comparator]) -> str:
        self._validate_func(func)
        return self.map_dict[func]

    def _format_datetime_comparison(self, comparison: Comparison) -> str:
        value = comparison.value
        attribute = comparison.attribute
        comparator = comparison.comparator

        date_string = value.get("date", "")
        date_object = datetime.strptime(date_string, "%Y-%m-%d")
        if comparator == Comparator.EQ:
            start_of_day = date_object.replace(hour=0, minute=0, second=0, microsecond=0)
            end_of_day = start_of_day + timedelta(days=1) - timedelta(microseconds=1)
            start_timestamp = start_of_day.timestamp()
            end_timestamp = end_of_day.timestamp()
            return f" ({attribute} >= {start_timestamp} AND {attribute} <= {end_timestamp}) "
        else:
            timestamp = date_object.timestamp()
            return f"{attribute}{self._format_func(comparator)}{timestamp}"

    def visit_operation(self, operation: Operation) -> Dict:
        args = [arg.accept(self) for arg in operation.arguments]
        return f"({f' {operation.operator.value} '.join(args)})"

    def visit_comparison(self, comparison: Comparison) -> Dict:
        value = comparison.value
        attribute = comparison.attribute

        if attribute == "timestamp":
            return self._format_datetime_comparison(comparison)
        if attribute == "groupname":
            return f'metadata matches "{attribute}==={value}"'

        return f'metadata matches "{attribute}==={replace_non_alphanumeric_with_wildcards(value)}"'

    def visit_structured_query(self, structured_query: StructuredQuery) -> Tuple[str, dict]:
        self.log.info(f"Structured query: {structured_query}")
        if structured_query.filter is None:
            kwargs = {}
        else:
            filter = structured_query.filter.accept(self)
            self.log.info(f"Structured filter: {filter}")
            kwargs = {"filter": filter}
        return structured_query.query, kwargs
