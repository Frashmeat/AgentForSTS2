from __future__ import annotations

import asyncio

from approval.runtime import get_approval_service, get_approval_store


class ApprovalFacadeService:
    def list_requests(self):
        store = get_approval_store()
        return [request.to_dict() for request in store.list_requests()]

    def get_request(self, action_id: str):
        return get_approval_store().get_request(action_id).to_dict()

    def approve_request(self, action_id: str):
        return get_approval_store().approve_request(action_id).to_dict()

    def reject_request(self, action_id: str, reason: str):
        return get_approval_store().reject_request(action_id, reason).to_dict()

    def execute_request(self, action_id: str):
        return asyncio.run(get_approval_service().execute_request(action_id)).to_dict()
