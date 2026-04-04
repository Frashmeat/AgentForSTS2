"""认证模块 ORM 模型包。"""

from .email_verification import EmailVerificationRecord
from .user import UserRecord


def auth_tables():
    return [
        UserRecord.__table__,
        EmailVerificationRecord.__table__,
    ]


__all__ = [
    "EmailVerificationRecord",
    "UserRecord",
    "auth_tables",
]
