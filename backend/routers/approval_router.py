from __future__ import annotations

import asyncio

from fastapi import APIRouter, HTTPException, Request

from approval.runtime import get_approval_service, get_approval_store

router = APIRouter(prefix="/approvals")


def _approval_facade(request):
    if request is None:
        return None
    container = getattr(getattr(request.app.state, "container", None), "resolve_optional_singleton", None)
    if container is None:
        return None
    flags = getattr(request.app.state.container, "platform_migration_flags", None)
    if flags is None or not getattr(flags, "platform_service_split_enabled", False):
        return None
    return request.app.state.container.resolve_optional_singleton("platform.approval_facade_service")


@router.get("")
def list_approvals(request: Request = None):
    facade = _approval_facade(request)
    if facade is not None:
        return facade.list_requests()
    store = get_approval_store()
    return [request.to_dict() for request in store.list_requests()]


@router.get("/{action_id}")
def get_approval(action_id: str, request: Request = None):
    facade = _approval_facade(request)
    if facade is not None:
        try:
            return facade.get_request(action_id)
        except KeyError as exc:
            raise HTTPException(status_code=404, detail="Approval request not found") from exc
    store = get_approval_store()
    try:
        return store.get_request(action_id).to_dict()
    except KeyError as exc:
        raise HTTPException(status_code=404, detail="Approval request not found") from exc


@router.post("/{action_id}/approve")
def approve_approval(action_id: str, request: Request = None):
    facade = _approval_facade(request)
    if facade is not None:
        try:
            return facade.approve_request(action_id)
        except KeyError as exc:
            raise HTTPException(status_code=404, detail="Approval request not found") from exc
    store = get_approval_store()
    try:
        return store.approve_request(action_id).to_dict()
    except KeyError as exc:
        raise HTTPException(status_code=404, detail="Approval request not found") from exc


@router.post("/{action_id}/reject")
def reject_approval(action_id: str, body: dict, request: Request = None):
    facade = _approval_facade(request)
    if facade is not None:
        try:
            return facade.reject_request(action_id, body.get("reason", ""))
        except KeyError as exc:
            raise HTTPException(status_code=404, detail="Approval request not found") from exc
    store = get_approval_store()
    try:
        return store.reject_request(action_id, body.get("reason", "")).to_dict()
    except KeyError as exc:
        raise HTTPException(status_code=404, detail="Approval request not found") from exc


@router.post("/{action_id}/execute")
def execute_approval(action_id: str, request: Request = None):
    facade = _approval_facade(request)
    if facade is not None:
        store = get_approval_store()
        try:
            action = store.get_request(action_id)
        except KeyError as exc:
            raise HTTPException(status_code=404, detail="Approval request not found") from exc
        if action.requires_approval and action.status != "approved":
            raise HTTPException(status_code=409, detail="Approval request must be approved before execution")
        try:
            return facade.execute_request(action_id)
        except Exception as exc:
            raise HTTPException(status_code=500, detail=str(exc)) from exc
    store = get_approval_store()
    service = get_approval_service()
    try:
        action = store.get_request(action_id)
    except KeyError as exc:
        raise HTTPException(status_code=404, detail="Approval request not found") from exc

    if action.requires_approval and action.status != "approved":
        raise HTTPException(status_code=409, detail="Approval request must be approved before execution")

    try:
        updated = asyncio.run(service.execute_request(action_id))
        return updated.to_dict()
    except Exception as exc:
        raise HTTPException(status_code=500, detail=str(exc)) from exc
