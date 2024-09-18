import re

from onyx.catalog.ingest.base.types import Interval


def merge_overlap(arr: list[Interval]) -> list[Interval]:
    # Sort intervals based on start values
    arr.sort(key=lambda x: x.start)
    res_idx = 0  # Index of the last merged interval
    for i in range(1, len(arr)):
        # If current interval overlaps with
        # the last merged interval
        if arr[res_idx].end >= arr[i].start:
            arr[res_idx].end = max(arr[res_idx].end, arr[i].end)
        else:
            # Move to the next interval
            res_idx += 1
            arr[res_idx] = arr[i]
    return arr


def split_interval(interval: Interval, freq: int) -> list[Interval]:
    results: list[Interval] = []
    for start in range(interval.start, interval.end, freq):
        results.append(Interval(start=start, end=min(start + freq, interval.end)))
    return results


def clean_ascii_control_chars(text: str):
    return re.sub(r"[\x00-\x1F]+", "", text)


def clean_non_ascii_chars(text: str) -> str:
    """Cleans non-ascii characters from unicode string.

    Example
    -------
    \x88This text contains non-ascii characters!\x88
        -> This text contains non-ascii characters!
    """
    en = text.encode("ascii", "ignore")
    return en.decode()
