"""平台模式 ORM 模型包。"""

from app.modules.platform.domain.models.enums import AIExecutionStatus, JobItemStatus, JobStatus

from .ai_execution import AIExecutionRecord
from .artifact import ArtifactRecord
from .credential_health_check import CredentialHealthCheckRecord
from .execution_charge import ChargeStatus, ExecutionChargeRecord
from .execution_profile import ExecutionProfileRecord
from .job import JobRecord
from .job_event import JobEventRecord
from .job_item import JobItemRecord
from .platform_runtime_audit_event import PlatformRuntimeAuditEventRecord
from .quota_account import QuotaAccountRecord, QuotaAccountStatus
from .quota_balance import QuotaBalanceRecord
from .quota_bucket import QuotaBucketRecord, QuotaBucketType
from .server_credential import ServerCredentialRecord
from .usage_ledger import LedgerType, UsageLedgerRecord
from .user_platform_preference import UserPlatformPreferenceRecord


def platform_tables():
    return [
        JobRecord.__table__,
        JobItemRecord.__table__,
        AIExecutionRecord.__table__,
        ExecutionProfileRecord.__table__,
        ServerCredentialRecord.__table__,
        CredentialHealthCheckRecord.__table__,
        UserPlatformPreferenceRecord.__table__,
        ExecutionChargeRecord.__table__,
        QuotaAccountRecord.__table__,
        QuotaBalanceRecord.__table__,
        QuotaBucketRecord.__table__,
        UsageLedgerRecord.__table__,
        ArtifactRecord.__table__,
        JobEventRecord.__table__,
        PlatformRuntimeAuditEventRecord.__table__,
    ]


__all__ = [
    "AIExecutionRecord",
    "AIExecutionStatus",
    "ArtifactRecord",
    "ChargeStatus",
    "CredentialHealthCheckRecord",
    "ExecutionChargeRecord",
    "ExecutionProfileRecord",
    "JobEventRecord",
    "JobItemRecord",
    "JobItemStatus",
    "JobRecord",
    "JobStatus",
    "LedgerType",
    "PlatformRuntimeAuditEventRecord",
    "QuotaAccountRecord",
    "QuotaAccountStatus",
    "QuotaBalanceRecord",
    "QuotaBucketRecord",
    "QuotaBucketType",
    "ServerCredentialRecord",
    "UsageLedgerRecord",
    "UserPlatformPreferenceRecord",
    "platform_tables",
]
