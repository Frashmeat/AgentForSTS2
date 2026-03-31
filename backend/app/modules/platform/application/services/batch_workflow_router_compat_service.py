from __future__ import annotations


class BatchWorkflowRouterCompatService:
    def plan(self, body: dict):
        from routers import batch_workflow as batch_router

        return batch_router._legacy_api_plan(body)

    async def handle_ws_batch(self, ws) -> None:
        from routers import batch_workflow as batch_router

        await batch_router._handle_legacy_ws_batch(ws)
