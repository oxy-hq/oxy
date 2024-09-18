from enum import StrEnum


class Stage(StrEnum):
    local = "local"
    dev = "dev"
    localtest = "localtest"
    prod = "prod"
    demo = "demo"

    def is_local(self):
        return self == Stage.local or self == Stage.localtest
