"""平台模式 ORM 模型包。"""

from .ai_execution import AIExecutionRecord
from .artifact import ArtifactRecord
from .execution_charge import ChargeStatus, ExecutionChargeRecord
from .job import JobRecord
from .job_event import JobEventRecord
from .job_item import JobItemRecord
from .quota_account import QuotaAccountRecord, QuotaAccountStatus
from .quota_bucket import QuotaBucketRecord, QuotaBucketType
from .usage_ledger import LedgerType, UsageLedgerRecord
from app.modules.platform.domain.models.enums import AIExecutionStatus, JobItemStatus, JobStatus


def platform_tables():
    return [
        JobRecord.__table__,
        JobItemRecord.__table__,
        AIExecutionRecord.__table__,
        ExecutionChargeRecord.__table__,
        QuotaAccountRecord.__table__,
        QuotaBucketRecord.__table__,
        UsageLedgerRecord.__table__,
        ArtifactRecord.__table__,
        JobEventRecord.__table__,
    ]


__all__ = [
    "AIExecutionRecord",
    "ArtifactRecord",
    "ChargeStatus",
    "ExecutionChargeRecord",
    "AIExecutionStatus",
    "JobEventRecord",
    "JobItemStatus",
    "JobItemRecord",
    "JobRecord",
    "JobStatus",
    "LedgerType",
    "QuotaAccountRecord",
    "QuotaAccountStatus",
    "QuotaBucketRecord",
    "QuotaBucketType",
    "UsageLedgerRecord",
    "platform_tables",
]
