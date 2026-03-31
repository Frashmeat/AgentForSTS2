from pathlib import Path


def test_main_keeps_approval_and_analyzer_route_registrations():
    source = Path("backend/main.py").read_text(encoding="utf-8")

    assert "from routers.log_analyzer import router as log_analyzer_router" in source
    assert "from routers.mod_analyzer import router as mod_analyzer_router" in source
    assert "from routers.approval_router import router as approval_router" in source
    assert 'app.include_router(log_analyzer_router, prefix="/api")' in source
    assert 'app.include_router(mod_analyzer_router,  prefix="/api")' in source
    assert 'app.include_router(approval_router,      prefix="/api")' in source


def test_config_router_keeps_image_test_entrypoint():
    source = Path("backend/routers/config_router.py").read_text(encoding="utf-8")

    assert '@router.get("/test_imggen")' in source
    assert "from image.generator import generate_images" in source
    assert 'imgs = await generate_images(_TEXT_LOADER.load("runtime_system.config_image_test_prompt").strip(), "power", batch_size=1)' in source
