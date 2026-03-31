from __future__ import annotations


class WorkflowRouterCompatService:
    async def handle_ws_create(self, ws) -> None:
        from routers import workflow as workflow_router

        await workflow_router._handle_legacy_ws_create(ws)

    async def create_project(self, body: dict):
        from routers import workflow as workflow_router

        return await workflow_router._legacy_api_create_project(body)

    async def build_project(self, body: dict):
        from routers import workflow as workflow_router

        return await workflow_router._legacy_api_build(body)

    async def package_project(self, body: dict):
        from routers import workflow as workflow_router

        return await workflow_router._legacy_api_package(body)
