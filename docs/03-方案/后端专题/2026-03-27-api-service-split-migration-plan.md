# API / Service Split Migration Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Incrementally migrate the current router-driven FastAPI backend to an API / Service / Runner / Repository structure for platform-mode jobs without a one-shot rewrite.

**Architecture:** Keep the existing FastAPI monolith and current HTTP/WebSocket entrypoints, but move orchestration and persistence concerns behind application services and repositories. Introduce the `job -> job_item -> ai_execution` chain first, then route existing workflow, batch, build/deploy, config, and approval flows through transport contracts plus feature flags so rollout can be gradual and reversible.

**Tech Stack:** Python, FastAPI, Uvicorn, Pydantic-style DTOs/contracts, existing application container, existing settings-based migration flags, SQL migration tooling used by the backend, pytest

## Decision Freeze

- Phase-1 scope includes `admin` APIs; they are not deferred behind the user API migration.
- First-release status enums are frozen to the fine-grained set defined in the backend design baseline; implementation and migrations should use those enums directly.
- Dual-path migration is frozen to `dual-write + read by flag`.
- Phase-1 read authority remains legacy read models (`read legacy`) until parity checks and rollback tests pass; write authority is dual-written during the migration window.
- The v1 refund matrix is frozen for implementation. Do not reopen refund semantics for phase-1 except by explicit design review.

## Decision Gates

- Gate A: Do not start schema work until the fine-grained enums and v1 refund matrix are treated as frozen inputs.
- Gate B: Do not enable any new read path until dual-write parity checks pass against legacy responses for list/detail/items/events/quota/admin views.
- Gate C: Do not switch router delegation defaults until rollback tests pass for legacy-only, dual-write with `read legacy`, and split-read modes.
- Gate D: Do not enable the unified event/WebSocket contract until polling paths are stable on the new read model.

---

## File Map

- `backend/main.py`
  Responsibility: compose routers; eventually register platform APIs separately from legacy workflow endpoints.
- `backend/routers/workflow.py`
  Responsibility today: single-item transport + orchestration + execution + event streaming; target state is thin transport adapter.
- `backend/routers/batch_workflow.py`
  Responsibility today: batch transport + planning/orchestration + concurrency control; target state is thin transport adapter.
- `backend/routers/build_deploy.py`
  Responsibility today: build/deploy transport with embedded execution flow; target state is build transport calling an application service / runner facade.
- `backend/routers/config_router.py`
  Responsibility today: config transport; target state is settings/query facade and migration toggle entrypoint.
- `backend/routers/approval_router.py`
  Responsibility today: approval transport; target state is approval application service facade.
- `backend/app/composition/container.py`
  Responsibility: singleton/provider composition root; will become the wiring point for repositories, services, runners, and rollout toggles.
- `backend/app/shared/infra/config/settings.py`
  Responsibility: normalized settings + migration flags; extend to support split rollout switches.
- `backend/app/modules/platform/contracts/*.py`
  Responsibility: user/admin transport DTOs, event payloads, runner step contracts.
- `backend/app/modules/platform/application/services/*.py`
  Responsibility: job lifecycle, orchestration, query, quota/billing, approval/build facades.
- `backend/app/modules/platform/domain/repositories/*.py`
  Responsibility: repository interfaces aligned to the confirmed job chain and query boundaries.
- `backend/app/modules/platform/infra/persistence/models/*.py`
  Responsibility: ORM entities / table mappings for `jobs`, `job_items`, `ai_executions`, quota, events, artifacts, charges.
- `backend/app/modules/platform/infra/persistence/repositories/*.py`
  Responsibility: repository implementations for write and read models.
- `backend/app/modules/platform/runner/*.py`
  Responsibility: workflow registry, step dispatcher, execution adapters, build/deploy bridge, transport-independent execution flow.
- `backend/migrations/*`
  Responsibility: schema rollout for main chain, quota/billing chain, event chain, artifacts, compatibility columns, and backfill guards.
- `backend/tests/platform/*`
  Responsibility: repository, service, runner, router-contract, and migration regression coverage.

## Dual-Path Authority Model

- Phase 1: legacy database reads are the source of truth for API responses; new platform tables receive dual writes for backfill and parity validation.
- Phase 2: selected APIs switch to new reads behind flags after parity passes, while dual-write stays on for rollback safety.
- Phase 3: legacy read paths are retired only after split-read production soak plus rollback rehearsal succeed.
- At no phase should two independent write paths mutate the same logical field with different business rules; service-layer write semantics must stay single-sourced even when persistence is dual-written.

## Chunk 1: Contracts And Persistence Backbone

### Task 1: Introduce platform transport contracts and migration toggles

**Files:**
- Modify: `backend/app/shared/infra/config/settings.py`
- Modify: `backend/app/composition/container.py`
- Modify: `backend/main.py`
- Create: `backend/app/modules/platform/contracts/job_commands.py`
- Create: `backend/app/modules/platform/contracts/job_queries.py`
- Create: `backend/app/modules/platform/contracts/events.py`
- Create: `backend/app/modules/platform/contracts/runner_contracts.py`
- Create: `backend/tests/platform/contracts/test_platform_contracts.py`
- Create: `backend/tests/platform/config/test_platform_migration_flags.py`

- [ ] **Step 1: Define rollout switches for split APIs and services**
  Add normalized settings fields for `platform_jobs_api_enabled`, `platform_service_split_enabled`, `platform_runner_enabled`, and `platform_events_v1_enabled`, keeping existing migration flags intact.

- [ ] **Step 2: Register contract and service placeholders in the composition root**
  Extend `ApplicationContainer` wiring so later tasks can resolve platform repositories, services, and runner adapters without changing router code again.

- [ ] **Step 3: Create transport contracts for create/start/cancel/list/detail/events/quota/admin views**
  Add request/response DTO modules that separate user-visible `job` / `job_item` payloads from internal `ai_execution` payloads.

- [ ] **Step 4: Create a versioned event contract**
  Define one event payload model shared by polling and future WebSocket transport, with optional admin-only execution fields.

- [ ] **Step 5: Gate router registration in `main.py`**
  Keep current routers mounted, but add the switch points needed to later register platform routers beside or in front of legacy endpoints.

- [ ] **Step 6: Write contract and settings tests**
  Verify DTO serialization, admin/user visibility boundaries, and default-off behavior for new migration flags.

**Tests:**
- `backend/tests/platform/contracts/test_platform_contracts.py`
- `backend/tests/platform/config/test_platform_migration_flags.py`

**Commands:**
- `pytest backend/tests/platform/contracts/test_platform_contracts.py -q`
- `pytest backend/tests/platform/config/test_platform_migration_flags.py -q`

**Expected:**
- Contract modules compile and serialize deterministically.
- New settings default to compatibility mode.
- Container wiring can resolve placeholders without breaking current startup.

### Task 2: Add persistence schema for the platform job chain

**Files:**
- Create: `backend/migrations/versions/20260327_01_create_platform_job_chain.py`
- Create: `backend/app/modules/platform/infra/persistence/models/job.py`
- Create: `backend/app/modules/platform/infra/persistence/models/job_item.py`
- Create: `backend/app/modules/platform/infra/persistence/models/ai_execution.py`
- Create: `backend/app/modules/platform/infra/persistence/models/execution_charge.py`
- Create: `backend/app/modules/platform/infra/persistence/models/quota_account.py`
- Create: `backend/app/modules/platform/infra/persistence/models/quota_bucket.py`
- Create: `backend/app/modules/platform/infra/persistence/models/usage_ledger.py`
- Create: `backend/app/modules/platform/infra/persistence/models/artifact.py`
- Create: `backend/app/modules/platform/infra/persistence/models/job_event.py`
- Create: `backend/tests/platform/migrations/test_platform_job_chain_schema.py`

- [ ] **Step 1: Encode the confirmed table set and status enums in a migration**
  Create tables for `jobs`, `job_items`, `ai_executions`, `execution_charges`, `quota_accounts`, `quota_buckets`, `usage_ledgers`, `artifacts`, and `job_events`, using the frozen fine-grained v1 enums directly.

- [ ] **Step 2: Implement ORM/entity mappings**
  Add one focused model file per aggregate/fact table, including soft-delete/archive fields and timestamp conventions from the design docs.

- [ ] **Step 3: Add the scoped partial unique constraint for execution idempotency**
  Enforce `user_id + job_item_id + request_idempotency_key` uniqueness only when the idempotency key is non-null.

- [ ] **Step 4: Add indexes for the first-release query paths**
  Cover user job list/detail, item pagination, latest execution lookup, bucket lookup, and event stream cursor reads.

- [ ] **Step 5: Add migration verification coverage**
  Assert tables, indexes, and unique constraints exist and match the agreed semantics.

**Tests:**
- `backend/tests/platform/migrations/test_platform_job_chain_schema.py`

**Commands:**
- `pytest backend/tests/platform/migrations/test_platform_job_chain_schema.py -q`
- `alembic upgrade head`

**Expected:**
- The schema supports the full `job -> job_item -> ai_execution` chain plus quota, billing, artifacts, and events.
- Idempotency and query indexes are enforced in the database, not only in application code.

### Task 3: Implement repository interfaces and first-pass persistence adapters

**Files:**
- Create: `backend/app/modules/platform/domain/repositories/job_repository.py`
- Create: `backend/app/modules/platform/domain/repositories/job_query_repository.py`
- Create: `backend/app/modules/platform/domain/repositories/quota_query_repository.py`
- Create: `backend/app/modules/platform/domain/repositories/ai_execution_repository.py`
- Create: `backend/app/modules/platform/domain/repositories/execution_charge_repository.py`
- Create: `backend/app/modules/platform/domain/repositories/quota_account_repository.py`
- Create: `backend/app/modules/platform/domain/repositories/usage_ledger_repository.py`
- Create: `backend/app/modules/platform/domain/repositories/artifact_repository.py`
- Create: `backend/app/modules/platform/domain/repositories/job_event_repository.py`
- Create: `backend/app/modules/platform/domain/repositories/admin_query_repositories.py`
- Create: `backend/app/modules/platform/infra/persistence/repositories/*.py`
- Modify: `backend/app/composition/container.py`
- Create: `backend/tests/platform/repositories/test_job_repository.py`
- Create: `backend/tests/platform/repositories/test_execution_and_quota_repositories.py`
- Create: `backend/tests/platform/repositories/test_job_query_repositories.py`

- [ ] **Step 1: Turn the repository draft into code interfaces**
  Create protocol/ABC-style repository definitions that mirror the agreed read/write split and keep `AIExecution` out of user-facing repositories.

- [ ] **Step 2: Implement write repositories first**
  Prioritize `JobRepository`, `AIExecutionRepository`, `ExecutionChargeRepository`, `QuotaAccountRepository`, `UsageLedgerRepository`, and `JobEventRepository`.

- [ ] **Step 3: Implement read-model repositories**
  Add user query repositories for list/detail/items/events/quota plus admin query repositories for executions, billing, and event/audit views.

- [ ] **Step 4: Register implementations in the container**
  Wire concrete persistence adapters behind stable keys so services can swap implementations later if needed.

- [ ] **Step 5: Cover transaction-critical repository behavior**
  Test row locking helpers, idempotency lookup, quota bucket selection, event append ordering, and user/admin visibility separation.

**Tests:**
- `backend/tests/platform/repositories/test_job_repository.py`
- `backend/tests/platform/repositories/test_execution_and_quota_repositories.py`
- `backend/tests/platform/repositories/test_job_query_repositories.py`

**Commands:**
- `pytest backend/tests/platform/repositories/test_job_repository.py -q`
- `pytest backend/tests/platform/repositories/test_execution_and_quota_repositories.py -q`
- `pytest backend/tests/platform/repositories/test_job_query_repositories.py -q`

**Expected:**
- Service-layer code can load/update aggregates without direct SQL in routers.
- Query paths no longer need to reuse write models.

## Chunk 2: Services And Runner Extraction

### Task 4: Build application services around the confirmed lifecycle

**Files:**
- Create: `backend/app/modules/platform/application/services/job_application_service.py`
- Create: `backend/app/modules/platform/application/services/job_query_service.py`
- Create: `backend/app/modules/platform/application/services/execution_orchestrator_service.py`
- Create: `backend/app/modules/platform/application/services/quota_billing_service.py`
- Create: `backend/app/modules/platform/application/services/event_service.py`
- Create: `backend/app/modules/platform/application/services/admin_query_service.py`
- Create: `backend/app/modules/platform/application/services/approval_service.py`
- Create: `backend/app/modules/platform/application/services/build_deploy_service.py`
- Modify: `backend/app/composition/container.py`
- Create: `backend/tests/platform/services/test_job_application_service.py`
- Create: `backend/tests/platform/services/test_execution_orchestrator_service.py`
- Create: `backend/tests/platform/services/test_job_query_service.py`
- Create: `backend/tests/platform/services/test_quota_billing_service.py`

- [ ] **Step 1: Implement `JobApplicationService`**
  Support create-draft, start, and cancel flows with explicit transaction boundaries and event emission.

- [ ] **Step 2: Implement `QuotaBillingService`**
  Encapsulate reserve / capture / refund rules so routers and runners never touch ledger logic directly, and implement only the frozen v1 refund matrix in phase 1.

- [ ] **Step 3: Implement `ExecutionOrchestratorService`**
  Create the service responsible for checking cancellation/quota, creating executions, updating job aggregates, and delegating actual work to the runner layer.

- [ ] **Step 4: Implement query services**
  Add `JobQueryService` and `AdminQueryService` to shape user/admin responses from read repositories plus event visibility rules.

- [ ] **Step 5: Add approval/build facades**
  Wrap current approval/build flows so `approval_router.py` and `build_deploy.py` can become transport-only callers.

- [ ] **Step 6: Register services in the container and add service tests**
  Cover draft-to-running transitions, quota exhaustion, cancellation semantics, refund decisions, and admin/user data separation.

**Tests:**
- `backend/tests/platform/services/test_job_application_service.py`
- `backend/tests/platform/services/test_execution_orchestrator_service.py`
- `backend/tests/platform/services/test_job_query_service.py`
- `backend/tests/platform/services/test_quota_billing_service.py`

**Commands:**
- `pytest backend/tests/platform/services/test_job_application_service.py -q`
- `pytest backend/tests/platform/services/test_execution_orchestrator_service.py -q`
- `pytest backend/tests/platform/services/test_job_query_service.py -q`
- `pytest backend/tests/platform/services/test_quota_billing_service.py -q`

**Expected:**
- Router code can delegate lifecycle rules to services.
- Quota and refund behavior becomes reusable and testable outside HTTP/WebSocket handlers.

### Task 5: Extract workflow runner abstractions from legacy WebSocket flows

**Files:**
- Create: `backend/app/modules/platform/runner/workflow_registry.py`
- Create: `backend/app/modules/platform/runner/workflow_runner.py`
- Create: `backend/app/modules/platform/runner/step_dispatcher.py`
- Create: `backend/app/modules/platform/runner/execution_adapter.py`
- Create: `backend/app/modules/platform/runner/build_deploy_adapter.py`
- Create: `backend/app/modules/platform/runner/approval_adapter.py`
- Modify: `backend/app/composition/container.py`
- Create: `backend/tests/platform/runner/test_workflow_registry.py`
- Create: `backend/tests/platform/runner/test_workflow_runner.py`
- Create: `backend/tests/platform/runner/test_execution_adapter.py`

- [ ] **Step 1: Define workflow registry and step-dispatch boundaries**
  Map `job_type` / `item_type` to shared workflow definitions and explicit step payload contracts.

- [ ] **Step 2: Extract execution adapters around current callable assets**
  Wrap existing image/code/build/approval execution chains behind runner interfaces so application services do not import router helpers.

- [ ] **Step 3: Implement a runner that reports into the unified event model**
  Ensure polling and future WebSocket delivery both consume the same emitted stage/event structure.

- [ ] **Step 4: Add compatibility shims**
  Keep legacy flows callable while the new orchestrator and runner are validated behind switches.

- [ ] **Step 5: Test runner behavior**
  Cover workflow selection, step dispatch, failure mapping to `failed_business` / `failed_system`, and event emission ordering.

**Tests:**
- `backend/tests/platform/runner/test_workflow_registry.py`
- `backend/tests/platform/runner/test_workflow_runner.py`
- `backend/tests/platform/runner/test_execution_adapter.py`

**Commands:**
- `pytest backend/tests/platform/runner/test_workflow_registry.py -q`
- `pytest backend/tests/platform/runner/test_workflow_runner.py -q`
- `pytest backend/tests/platform/runner/test_execution_adapter.py -q`

**Expected:**
- The platform execution flow becomes transport-independent.
- Existing workflow assets remain reusable instead of being rewritten.

## Chunk 3: Router Migration And Rollout

### Task 6: Add platform job APIs beside legacy routes

**Files:**
- Create: `backend/routers/platform_jobs.py`
- Create: `backend/routers/platform_admin.py`
- Modify: `backend/main.py`
- Modify: `backend/app/composition/container.py`
- Create: `backend/tests/platform/routers/test_platform_jobs_http.py`
- Create: `backend/tests/platform/routers/test_platform_admin_http.py`

- [ ] **Step 1: Implement user-facing HTTP handlers**
  Add `POST /api/platform/jobs`, `POST /api/platform/jobs/{jobId}/start`, `POST /api/platform/jobs/{jobId}/cancel`, `GET /api/platform/jobs`, `GET /api/platform/jobs/{jobId}`, `GET /api/platform/jobs/{jobId}/items`, `GET /api/platform/jobs/{jobId}/events`, and `GET /api/platform/quota`.

- [ ] **Step 2: Implement admin-facing HTTP handlers**
  Add execution, billing/refund, and audit endpoints that expose admin-only read models without leaking them into user APIs; `admin` scope is phase-1 baseline, not a later rollout item.

- [ ] **Step 3: Resolve services from the container**
  Keep handlers thin: auth/context parsing, DTO conversion, service call, DTO response.

- [ ] **Step 4: Gate router registration with settings**
  Mount new platform routers only when the rollout flags are enabled, without removing legacy routes yet.

- [ ] **Step 5: Add API tests**
  Cover happy path, cancelled-before-start, quota-exhausted, missing-resource, and admin visibility checks.

**Tests:**
- `backend/tests/platform/routers/test_platform_jobs_http.py`
- `backend/tests/platform/routers/test_platform_admin_http.py`

**Commands:**
- `pytest backend/tests/platform/routers/test_platform_jobs_http.py -q`
- `pytest backend/tests/platform/routers/test_platform_admin_http.py -q`

**Expected:**
- A clean platform API exists without forcing an immediate cutover of current routes.
- User/admin contracts are enforced at the transport boundary.

### Task 7: Thin down legacy routers by delegating to services and runner facades

**Files:**
- Modify: `backend/routers/workflow.py`
- Modify: `backend/routers/batch_workflow.py`
- Modify: `backend/routers/build_deploy.py`
- Modify: `backend/routers/config_router.py`
- Modify: `backend/routers/approval_router.py`
- Modify: `backend/app/composition/container.py`
- Modify: `backend/app/shared/infra/config/settings.py`
- Create: `backend/tests/platform/routers/test_workflow_router_compat.py`
- Create: `backend/tests/platform/routers/test_batch_router_compat.py`
- Create: `backend/tests/platform/routers/test_build_deploy_router_compat.py`
- Create: `backend/tests/platform/routers/test_approval_and_config_router_compat.py`

- [ ] **Step 1: Replace embedded orchestration in `workflow.py`**
  Move lifecycle, event shaping, and execution-start decisions into platform services/runner while preserving the current WebSocket contract until the unified contract flag is on.

- [ ] **Step 2: Replace embedded orchestration in `batch_workflow.py`**
  Keep existing batch UX, but move plan execution, approval, quota checks, and item state transitions into services and runner adapters.

- [ ] **Step 3: Delegate build/deploy and approval routes**
  Make `build_deploy.py` and `approval_router.py` call service facades instead of directly touching runtime internals.

- [ ] **Step 4: Turn config routes into settings/query endpoints**
  Use `config_router.py` to surface rollout status and mutable settings, not to coordinate business logic.

- [ ] **Step 5: Add compatibility coverage**
  Verify legacy routes still work with flags off, and verify split-path delegation with flags on.

**Tests:**
- `backend/tests/platform/routers/test_workflow_router_compat.py`
- `backend/tests/platform/routers/test_batch_router_compat.py`
- `backend/tests/platform/routers/test_build_deploy_router_compat.py`
- `backend/tests/platform/routers/test_approval_and_config_router_compat.py`

**Commands:**
- `pytest backend/tests/platform/routers/test_workflow_router_compat.py -q`
- `pytest backend/tests/platform/routers/test_batch_router_compat.py -q`
- `pytest backend/tests/platform/routers/test_build_deploy_router_compat.py -q`
- `pytest backend/tests/platform/routers/test_approval_and_config_router_compat.py -q`

**Expected:**
- Legacy entrypoints remain usable during migration.
- Router files shrink toward transport-only responsibilities.

### Task 8: Roll out migration checkpoints, data backfill hooks, and end-to-end verification

**Files:**
- Create: `backend/migrations/versions/20260327_02_platform_backfill_and_switches.py`
- Create: `backend/app/modules/platform/application/services/migration_rollout_service.py`
- Modify: `backend/app/composition/container.py`
- Modify: `backend/app/shared/infra/config/settings.py`
- Modify: `backend/main.py`
- Create: `backend/tests/platform/e2e/test_platform_job_lifecycle.py`
- Create: `backend/tests/platform/e2e/test_platform_rollout_flags.py`
- Create: `docs/superpowers/checklists/platform-service-split-rollout.md`

- [ ] **Step 1: Add rollout/backfill service hooks**
  Create one service responsible for enabling flags in the correct order, validating schema readiness, enforcing the decision gates, and backfilling any compatibility metadata needed by new queries.

- [ ] **Step 2: Add a second migration for compatibility/backfill support**
  Include any view/materialized fields, nullable-to-required transitions, or default value backfills discovered while implementing services.

- [ ] **Step 3: Write end-to-end lifecycle tests**
  Cover create -> start -> execute -> finish, cancellation, quota exhaustion, refund flow, and event polling.

- [ ] **Step 4: Write rollout flag tests**
  Verify the system behaves correctly in legacy-only, dual-write with `read legacy`, and split-read modes.

- [ ] **Step 5: Add an operator checklist**
  Document the enablement order: schema first, dual-write wiring second, platform APIs third, router delegation fourth, split reads fifth, unified contract last.

- [ ] **Step 6: Add rollback test matrix**
  Explicitly verify rollback from split-read to `read legacy`, from router delegation back to legacy handlers, and from dual-write mode back to legacy-only read authority without data-loss or admin/user contract drift.

**Tests:**
- `backend/tests/platform/e2e/test_platform_job_lifecycle.py`
- `backend/tests/platform/e2e/test_platform_rollout_flags.py`

**Commands:**
- `pytest backend/tests/platform/e2e/test_platform_job_lifecycle.py -q`
- `pytest backend/tests/platform/e2e/test_platform_rollout_flags.py -q`
- `pytest backend/tests/platform -q`

**Expected:**
- Migration can be enabled in reversible phases instead of one cutover.
- Engineering and operations have a concrete checklist for rollout and rollback.

## Rollback Test Matrix

| Scenario | Starting mode | Rollback target | Must verify |
| --- | --- | --- | --- |
| Read-path rollback | dual-write + split-read | dual-write + `read legacy` | list/detail/items/events/quota/admin responses match legacy baselines |
| Router rollback | delegated legacy routers | legacy routers without service delegation | legacy HTTP/WebSocket contracts still hold; no orphan writes |
| Full migration pause | dual-write + `read legacy` | legacy-only mode | new writes can be paused cleanly; legacy reads remain correct |
| Admin surface rollback | split-read admin APIs | legacy-backed admin reads | execution/refund/audit visibility and auth boundaries remain unchanged |
| Event contract rollback | unified event contract on | polling-only legacy-compatible contract | cursor ordering, payload compatibility, and client fallback succeed |

## Rollout Order

1. Land contracts and settings flags with all new switches defaulted off.
2. Land schema and repository implementations with dual-write plumbing still disabled by default.
3. Land application services and runner abstractions, then enable dual-write while keeping all reads on legacy (`read legacy`).
4. Add new `/api/platform/*` user/admin APIs behind flags, still reading legacy-backed sources until parity is proven.
5. Delegate legacy routers to services/runner in compatibility mode while preserving `read legacy` as the default read authority.
6. Enable split reads by flag only after parity and rollback matrix pass.
7. Enable unified event/WebSocket contract only after split-read polling and compatibility tests pass.

## Open Decisions To Resolve During Execution

- Final enum freeze for `job_type`, `item_type`, and `event_type`.
- Exact persistence home for `workflow_version`, `step_protocol_version`, and `result_schema_version`.
- Whether phase-two runner isolation stays in-process or becomes a separate worker process after the split is stable.

Plan complete and saved to `docs/03-方案/后端专题/2026-03-27-api-service-split-migration-plan.md`. Ready to execute?
