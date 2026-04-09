from __future__ import annotations

from dataclasses import asdict, dataclass, field
from typing import Literal

from app.modules.planning.domain.models import ModPlan, PlanItem

ReviewStrictness = Literal["efficient", "balanced", "strict"]
PlanItemReviewStatus = Literal["clear", "needs_user_input", "invalid"]


@dataclass
class PlanValidationIssue:
    code: str
    message: str
    field: str = ""

    def to_dict(self) -> dict:
        return asdict(self)


@dataclass
class PlanItemValidation:
    item_id: str
    status: PlanItemReviewStatus
    issues: list[PlanValidationIssue] = field(default_factory=list)
    missing_fields: list[str] = field(default_factory=list)
    clarification_questions: list[str] = field(default_factory=list)

    def to_dict(self) -> dict:
        return {
            "item_id": self.item_id,
            "status": self.status,
            "issues": [issue.to_dict() for issue in self.issues],
            "missing_fields": list(self.missing_fields),
            "clarification_questions": list(self.clarification_questions),
        }


@dataclass
class PlanValidationResult:
    strictness: ReviewStrictness
    items: list[PlanItemValidation]

    def to_dict(self) -> dict:
        return {
            "strictness": self.strictness,
            "items": [item.to_dict() for item in self.items],
        }


def validate_plan(plan: ModPlan, strictness: ReviewStrictness = "balanced") -> PlanValidationResult:
    duplicate_ids = {
        item.id
        for item in plan.items
        if item.id and sum(1 for candidate in plan.items if candidate.id == item.id) > 1
    }
    known_ids = {item.id for item in plan.items}

    validations = [
        _validate_item(item, strictness=strictness, duplicate_ids=duplicate_ids, known_ids=known_ids)
        for item in plan.items
    ]
    return PlanValidationResult(strictness=strictness, items=validations)


def _validate_item(
    item: PlanItem,
    *,
    strictness: ReviewStrictness,
    duplicate_ids: set[str],
    known_ids: set[str],
) -> PlanItemValidation:
    issues: list[PlanValidationIssue] = []
    missing_fields: list[str] = []

    if not item.id.strip():
        issues.append(PlanValidationIssue(code="missing_id", message="item 缺少 id", field="id"))
    if not item.type:
        issues.append(PlanValidationIssue(code="missing_type", message="item 缺少 type", field="type"))
    if not item.name.strip():
        issues.append(PlanValidationIssue(code="missing_name", message="item 缺少 name", field="name"))
    if item.id in duplicate_ids:
        issues.append(PlanValidationIssue(code="duplicate_id", message="item id 重复", field="id"))
    if item.id in item.depends_on:
        issues.append(PlanValidationIssue(code="self_dependency", message="item 不能依赖自己", field="depends_on"))

    for dep in item.depends_on:
        if dep not in known_ids:
            issues.append(
                PlanValidationIssue(
                    code="missing_dependency",
                    message=f"依赖项不存在: {dep}",
                    field="depends_on",
                )
            )

    if item.type == "custom_code":
        if not item.goal.strip():
            missing_fields.append("goal")
        if not item.detailed_description.strip():
            missing_fields.append("detailed_description")
    elif strictness == "strict":
        if not item.goal.strip():
            missing_fields.append("goal")

    if strictness == "strict" and item.depends_on and not item.dependency_reason.strip():
        missing_fields.append("dependency_reason")

    if issues:
        return PlanItemValidation(item_id=item.id, status="invalid", issues=issues)

    if missing_fields:
        return PlanItemValidation(
            item_id=item.id,
            status="needs_user_input",
            issues=[
                PlanValidationIssue(
                    code="missing_detail",
                    message="item 仍缺少进入执行所需的关键信息",
                )
            ],
            missing_fields=missing_fields,
            clarification_questions=[_question_for_field(field_name) for field_name in missing_fields],
        )

    return PlanItemValidation(item_id=item.id, status="clear")


def _question_for_field(field_name: str) -> str:
    mapping = {
        "goal": "这个 item 在整个 Mod 中的目标是什么？",
        "detailed_description": "请补充这个 item 的详细行为说明。",
        "dependency_reason": "请说明它为什么依赖这些 item。",
    }
    return mapping.get(field_name, f"请补充字段：{field_name}")
