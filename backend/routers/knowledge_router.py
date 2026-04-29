from __future__ import annotations

from fastapi import APIRouter, HTTPException, Response

from app.modules.knowledge.infra import knowledge_runtime

router = APIRouter(prefix="/knowledge")


def _runtime():
    return knowledge_runtime


@router.get("/status")
def get_knowledge_status():
    try:
        return _runtime().get_knowledge_status()
    except HTTPException:
        raise
    except Exception as exc:
        raise HTTPException(status_code=500, detail=str(exc)) from exc


@router.post("/check")
def check_knowledge_status():
    try:
        return _runtime().check_knowledge_status()
    except HTTPException:
        raise
    except Exception as exc:
        raise HTTPException(status_code=500, detail=str(exc)) from exc


@router.post("/refresh/start")
def start_refresh_knowledge():
    try:
        return _runtime().start_refresh_task()
    except HTTPException:
        raise
    except Exception as exc:
        raise HTTPException(status_code=500, detail=str(exc)) from exc


@router.get("/refresh/latest")
def get_latest_refresh_knowledge():
    try:
        return _runtime().get_latest_refresh_task()
    except HTTPException:
        raise
    except Exception as exc:
        raise HTTPException(status_code=500, detail=str(exc)) from exc


@router.get("/export-pack")
def export_current_knowledge_pack():
    try:
        package = _runtime().export_current_knowledge_pack_zip()
    except HTTPException:
        raise
    except ValueError as exc:
        raise HTTPException(status_code=404, detail=str(exc)) from exc
    except Exception as exc:
        raise HTTPException(status_code=500, detail=str(exc)) from exc

    file_name = str(package.get("file_name", "workstation-current-knowledge-pack.zip"))
    return Response(
        content=package["content"],
        media_type="application/zip",
        headers={
            "Content-Disposition": f'attachment; filename="{file_name}"',
            "X-ATS-Knowledge-Pack-File-Count": str(package.get("file_count", "")),
        },
    )


@router.get("/refresh/{task_id}")
def get_refresh_knowledge(task_id: str):
    try:
        return _runtime().get_refresh_task(task_id)
    except HTTPException:
        raise
    except KeyError as exc:
        missing_task_id = exc.args[0] if exc.args else task_id
        raise HTTPException(status_code=404, detail=f"未找到知识库更新任务: {missing_task_id}") from exc
    except Exception as exc:
        raise HTTPException(status_code=500, detail=str(exc)) from exc
