"""通用数据库 Unit-of-Work context manager。

后台 service（worker、audit recorder 等）不在 HTTP 请求生命周期内，
没法用 routers/_auth_support.auth_session_scope，但仍需要一致的事务边界：
- 正常退出 → commit
- 异常 → rollback + 重抛
- finally → close

调用方持有一个 session_factory（callable[[], Session]），用 `with session_scope(factory) as session:`。

repository 一律 flush 不 commit；commit 由这个 scope 决定。
"""

from __future__ import annotations

from collections.abc import Callable
from contextlib import contextmanager

from sqlalchemy.orm import Session


@contextmanager
def session_scope(session_factory: Callable[[], Session]):
    session = session_factory()
    try:
        yield session
        session.commit()
    except Exception:
        session.rollback()
        raise
    finally:
        session.close()
