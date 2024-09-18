import re
from operator import attrgetter
from typing import Generic, Iterable, Literal, Sequence, TypeVar, cast

from onyx.shared.adapters.orm.schemas import BaseModel

NodeType = TypeVar("NodeType", bound=BaseModel)
ChildNodeType = TypeVar("ChildNodeType", bound=BaseModel)


def merge_attributes(
    target: NodeType,
    source: NodeType,
    exclude_fields: tuple[str, ...] = ("id", "created_at", "updated_at"),
) -> NodeType:
    updates = source.to_dict(exclude=set(exclude_fields))
    for key, value in updates.items():
        setattr(target, key, value)

    return source


def compute_changes(
    target: Sequence[NodeType],
    source: Iterable[NodeType],
    key_attr: str,
):
    get_key = attrgetter(key_attr)
    target_mapping = {get_key(item): item for item in target}
    source_mapping = {get_key(item): item for item in source}
    target_keys = set(target_mapping.keys())
    source_keys = set(source_mapping.keys())
    new_item_keys = source_keys - target_keys
    removed_item_keys = target_keys - source_keys
    updated_item_keys = target_keys.intersection(source_keys)
    new_items = (item for item in source if get_key(item) in new_item_keys)
    removed_items = (item for item in target[:] if get_key(item) in removed_item_keys)
    updated_pairs = ((target_mapping[key], source_mapping[key]) for key in updated_item_keys)
    return new_items, removed_items, updated_pairs


def string_is_quoted(string: str) -> bool:
    return string.startswith('"') and string.endswith('"')


class MergeMixin(Generic[NodeType]):
    __merge_exclude_fields__: tuple[str, ...] | Literal["__all__"] = (
        "id",
        "created_at",
        "updated_at",
    )
    __merge_children_configs__: tuple[tuple[str, str], ...] = ()

    def merge_children(self, source: Iterable[ChildNodeType], child_config: tuple[str, str]):
        field, key_attr = child_config
        child_getter: attrgetter[list[ChildNodeType]] = attrgetter(field)
        target = child_getter(self)
        if not isinstance(target, list):
            raise NotImplementedError("Only lists are supported for merge children")

        new_items, removed_items, updated_pairs = compute_changes(target, source, key_attr=key_attr)
        new_items_count = len(target)
        target.extend(new_items)
        new_items_count = len(target) - new_items_count

        removed_items_count = 0
        for item in removed_items:
            target.remove(item)
            removed_items_count += 1

        updated_item_count = 0
        for target_item, source_item in updated_pairs:
            if not isinstance(target_item, MergeMixin):
                raise NotImplementedError("Only MergeMixin items are supported for merge children")
            target_item.merge(source_item)
            updated_item_count += 1

        return new_items_count, removed_items_count, updated_item_count

    def merge(self, other: NodeType):
        if self.__merge_exclude_fields__ != "__all__":
            merge_attributes(
                cast(NodeType, self),
                other,
                exclude_fields=self.__merge_exclude_fields__,
            )

        for child_config in self.__merge_children_configs__:
            field, _ = child_config
            self.merge_children(getattr(other, field), child_config)

        return self


def canonicalize(text: str):
    return re.sub(r"[^\w\d_$]", "_", text.lower())
