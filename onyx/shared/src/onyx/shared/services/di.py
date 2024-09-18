from typing import Callable, TypeVar, cast, get_type_hints

from onyx.shared.logging import Logged
from onyx.shared.models.handlers import DependencyRegistration
from punq import Container, MissingDependencyError

T = TypeVar("T")
D = TypeVar("D")
R = TypeVar("R")


class DependenciesResolver(Logged):
    def __init__(self) -> None:
        self.container = Container()

    def register(self, *dependencies: DependencyRegistration[T]) -> None:
        if not self.container:
            raise RuntimeError("Cannot register dependencies without container")

        for registration in dependencies:
            if registration.is_instance:
                self.container.register(registration.dependency_type, instance=registration.dependency)
            else:
                self.container.register(registration.dependency_type, factory=registration.dependency)

    def resolve_param(self, param_type: type[T], **kwargs) -> T | None:
        if not self.container:
            raise RuntimeError("Cannot resolve dependencies without container")

        try:
            return cast(T, self.container.resolve(param_type, **kwargs))
        except MissingDependencyError:
            return None

    def resolve_dependencies(self, func: Callable[..., T], known_dependencies: dict[type[D], D]) -> dict[type[R], R]:
        annotations = get_type_hints(func)
        dependencies = {}
        for param_name, param_type in annotations.items():
            if param_name == "return":
                continue

            if param_type in known_dependencies:
                dependencies[param_name] = known_dependencies[param_type]
                continue

            resolved_param = self.resolve_param(param_type)
            if resolved_param is not None:
                dependencies[param_name] = resolved_param
        return dependencies
