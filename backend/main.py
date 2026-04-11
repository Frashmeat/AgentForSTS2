"""AgentTheSpire Backend — FastAPI 主入口"""
from app_factory import create_app

app = create_app("workstation")


if __name__ == "__main__":
    import uvicorn

    uvicorn.run("main:app", host="127.0.0.1", port=7860, reload=False)
