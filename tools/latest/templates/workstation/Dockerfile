FROM python:3.11-slim

ENV PYTHONDONTWRITEBYTECODE=1 \
    PYTHONUNBUFFERED=1 \
    PIP_NO_CACHE_DIR=1

WORKDIR /app

COPY backend/requirements.txt /tmp/requirements.txt

RUN python - <<'PY'
from pathlib import Path

source = Path("/tmp/requirements.txt")
target = Path("/tmp/requirements.docker.txt")
lines = []
needs_onnxruntime = False
for line in source.read_text(encoding="utf-8").splitlines():
    if line.startswith("rembg[gpu]"):
        lines.append(line.replace("rembg[gpu]", "rembg", 1))
        needs_onnxruntime = True
    else:
        lines.append(line)
if needs_onnxruntime and not any(line.startswith("onnxruntime") for line in lines):
    lines.append("onnxruntime>=1.18,<2.0")
target.write_text("\n".join(lines) + "\n", encoding="utf-8")
PY

RUN pip install --upgrade pip \
    && pip install -r /tmp/requirements.docker.txt

COPY backend /app/backend
COPY frontend /app/frontend
COPY mod_template /app/mod_template
COPY config.example.json /app/config.example.json

WORKDIR /app/backend

EXPOSE 7860

CMD ["sh", "-c", "uvicorn main_workstation:app --host 0.0.0.0 --port 7860"]
