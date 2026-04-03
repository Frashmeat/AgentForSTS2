"""AgentTheSpire Web Backend entrypoint."""
from app_factory import create_app

app = create_app("web")


if __name__ == "__main__":
    import uvicorn

    uvicorn.run("main_web:app", host="127.0.0.1", port=7870, reload=False)
