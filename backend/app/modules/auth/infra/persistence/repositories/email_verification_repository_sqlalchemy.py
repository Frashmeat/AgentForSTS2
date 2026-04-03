from __future__ import annotations

from sqlalchemy.orm import Session

from app.modules.auth.application.ports import EmailVerificationRepository
from app.modules.auth.domain.models import EmailVerificationTicket
from app.modules.auth.infra.persistence.models import EmailVerificationRecord


def _to_domain(record: EmailVerificationRecord) -> EmailVerificationTicket:
    return EmailVerificationTicket(
        verification_id=record.id,
        user_id=record.user_id,
        purpose=record.purpose,
        code=record.code,
        email=record.email,
        expires_at=record.expires_at,
        created_at=record.created_at,
        consumed_at=record.consumed_at,
    )


class EmailVerificationRepositorySqlAlchemy(EmailVerificationRepository):
    def __init__(self, session: Session) -> None:
        self.session = session

    def create_ticket(
        self,
        user_id: int,
        purpose: str,
        code: str,
        email: str,
        expires_at,
    ) -> EmailVerificationTicket:
        record = EmailVerificationRecord(
            user_id=user_id,
            purpose=purpose,
            code=code,
            email=email.strip().lower(),
            expires_at=expires_at,
        )
        self.session.add(record)
        self.session.flush()
        return _to_domain(record)

    def get_by_code(self, code: str, purpose: str) -> EmailVerificationTicket | None:
        record = (
            self.session.query(EmailVerificationRecord)
            .filter(
                EmailVerificationRecord.code == code,
                EmailVerificationRecord.purpose == purpose,
            )
            .one_or_none()
        )
        return _to_domain(record) if record is not None else None

    def save(self, ticket: EmailVerificationTicket) -> EmailVerificationTicket:
        record = (
            self.session.query(EmailVerificationRecord)
            .filter(EmailVerificationRecord.id == ticket.verification_id)
            .one_or_none()
        )
        if record is None:
            raise LookupError(f"email verification not found: {ticket.verification_id}")
        record.purpose = ticket.purpose
        record.code = ticket.code
        record.email = ticket.email
        record.expires_at = ticket.expires_at
        record.consumed_at = ticket.consumed_at
        self.session.flush()
        return _to_domain(record)
