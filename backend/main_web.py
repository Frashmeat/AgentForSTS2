"""AgentTheSpire Web Backend entrypoint."""

from app_factory import create_app

APP_ROLE = "web"
WEB_HOST = "127.0.0.1"
WEB_PORT = 7870

app = create_app(APP_ROLE)


if __name__ == "__main__":
    import uvicorn

    uvicorn.run("main_web:app", host=WEB_HOST, port=WEB_PORT, reload=False)
