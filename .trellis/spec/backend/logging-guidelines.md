# Logging Guidelines

> How logging is done in this project.

---

## Overview

Backend code uses Python's standard `logging` module. Each module should create a module logger:

```python
import logging

logger = logging.getLogger(__name__)
```

Logs are primarily used to connect long-running platform jobs across worker, orchestration, workflow, step handler, and external AI calls. Prefer stable business identifiers over free-form text so production failures can be traced quickly.

---

## Log Levels

- `debug`: noisy local details, such as low-level process command selection. Do not rely on it for production diagnosis.
- `info`: expected lifecycle events, including queue worker leader changes, execution route resolution, quota reserve/capture/refund, workflow start/finish, and step start/success.
- `warning`: expected but actionable problems, such as unsupported step types, quota blocks, upstream content-safety blocks, requeue/defer decisions, or classified external failures.
- `exception` / `error`: unexpected failures where a stack trace is useful. Use `logger.exception(...)` inside `except` blocks for unclassified system errors.

---

## Structured Logging

Use message templates with explicit fields:

```python
logger.info(
    "platform step start job_id=%s job_item_id=%s step_type=%s step_id=%s",
    request.job_id,
    request.job_item_id,
    request.step_type,
    request.step_id,
)
```

For Web AI execution, include these fields when available:

- `user_id`
- `job_id`
- `job_item_id`
- `execution_id`
- `job_type` / `item_type`
- `workflow_version`
- `step_type` / `step_id`
- `provider` / `model`
- `credential_ref`
- `retry_attempt`
- `switched`
- `reason_code`
- short `error` or `reason` text

Error summaries should be truncated before logging. A 300-500 character limit is enough for diagnosis without flooding logs.

---

## What to Log

- Queue worker leadership and tick failures.
- Execution idempotency hits.
- Execution route resolution, including provider/model and credential reference.
- Quota reserve, capture, refund, and quota-blocked decisions.
- Workflow start, resolved step list, retry scheduling, requeue/defer, and final status.
- Step start, success, unsupported type, and failure.
- External AI call start/success/failure using prompt length and output length, not full content.
- Classified upstream AI failures with `reason_code`, especially `upstream_request_blocked`.

For external AI errors, logs must make it possible to distinguish configuration errors, rate/credential failures, upstream content-safety blocks, and unknown system errors.

---

## What NOT to Log

- API keys, decrypted credentials, secrets, tokens, cookies, or authorization headers.
- Full prompt text, full generated output, full `input_payload`, or full model request/response bodies.
- User email, username, or other unnecessary personal data when `user_id` is enough.
- Filesystem paths that reveal secrets or private user content unless the path itself is the error being diagnosed.
- Unbounded exception text. Log a short summary and rely on stack traces for unexpected exceptions.

`credential_ref` is safe to log because it is an internal reference. `credential`, `secret`, and decrypted credential ciphertext are never safe to log.
