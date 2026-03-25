from dataclasses import dataclass


@dataclass(slots=True)
class ApprovalDecision:
    action: str
    approved: bool = False
    reason: str = ""
