from dataclasses import dataclass


USER_CANCELLED_CODE = "user_cancelled"
CLIENT_DISCONNECTED_CODE = "client_disconnected"


@dataclass(slots=True)
class DomainError(Exception):
    code: str
    message: str

    def __str__(self) -> str:
        return self.message


class WorkflowTermination(DomainError):
    """Expected workflow termination, not a system failure."""


def user_cancelled(message: str = "用户已取消当前生成") -> WorkflowTermination:
    return WorkflowTermination(code=USER_CANCELLED_CODE, message=message)


def client_disconnected(message: str = "客户端已断开，当前生成已停止") -> WorkflowTermination:
    return WorkflowTermination(code=CLIENT_DISCONNECTED_CODE, message=message)
