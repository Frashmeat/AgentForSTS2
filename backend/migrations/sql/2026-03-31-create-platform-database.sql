-- 平台后端数据库与表结构初始化 SQL
-- 用法：
-- 1. 使用 psql 连接到 postgres 或其他管理库执行本文件。
-- 2. 本文件会在数据库不存在时创建 agent_the_spire_platform。
-- 3. 随后通过 \connect 切换到目标库并创建平台主链表与索引。

\set ON_ERROR_STOP on

SELECT 'CREATE DATABASE agent_the_spire_platform TEMPLATE template0 ENCODING ''UTF8'''
WHERE NOT EXISTS (
    SELECT 1
    FROM pg_database
    WHERE datname = 'agent_the_spire_platform'
)\gexec

\connect agent_the_spire_platform

BEGIN;

CREATE TABLE IF NOT EXISTS quota_accounts (
    id BIGSERIAL NOT NULL,
    user_id BIGINT NOT NULL,
    status VARCHAR(9) NOT NULL,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT now() NOT NULL,
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT now() NOT NULL,
    CONSTRAINT pk_quota_accounts PRIMARY KEY (id),
    CONSTRAINT uq_quota_accounts_user_id UNIQUE (user_id),
    CONSTRAINT ck_quota_accounts_quota_account_status CHECK (status IN ('active', 'suspended', 'closed'))
);

CREATE TABLE IF NOT EXISTS jobs (
    id BIGSERIAL NOT NULL,
    user_id BIGINT NOT NULL,
    job_type VARCHAR(64) NOT NULL,
    status VARCHAR(17) NOT NULL,
    workflow_version VARCHAR(32) NOT NULL,
    input_summary TEXT NOT NULL,
    result_summary TEXT NOT NULL,
    error_summary TEXT NOT NULL,
    total_item_count INTEGER NOT NULL,
    pending_item_count INTEGER NOT NULL,
    running_item_count INTEGER NOT NULL,
    succeeded_item_count INTEGER NOT NULL,
    failed_business_item_count INTEGER NOT NULL,
    failed_system_item_count INTEGER NOT NULL,
    quota_skipped_item_count INTEGER NOT NULL,
    cancelled_before_start_item_count INTEGER NOT NULL,
    cancelled_after_start_item_count INTEGER NOT NULL,
    started_at TIMESTAMP WITH TIME ZONE,
    finished_at TIMESTAMP WITH TIME ZONE,
    cancel_requested_at TIMESTAMP WITH TIME ZONE,
    deleted_at TIMESTAMP WITH TIME ZONE,
    archived_at TIMESTAMP WITH TIME ZONE,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT now() NOT NULL,
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT now() NOT NULL,
    CONSTRAINT pk_jobs PRIMARY KEY (id),
    CONSTRAINT ck_jobs_job_status CHECK (status IN ('draft', 'queued', 'running', 'partial_succeeded', 'succeeded', 'failed', 'quota_exhausted', 'cancelling', 'cancelled'))
);

CREATE TABLE IF NOT EXISTS job_items (
    id BIGSERIAL NOT NULL,
    job_id BIGINT NOT NULL,
    user_id BIGINT NOT NULL,
    item_index INTEGER NOT NULL,
    item_type VARCHAR(64) NOT NULL,
    status VARCHAR(22) NOT NULL,
    input_summary TEXT NOT NULL,
    input_payload JSONB DEFAULT '{}'::jsonb NOT NULL,
    result_summary TEXT NOT NULL,
    error_summary TEXT NOT NULL,
    attempt_count INTEGER NOT NULL,
    started_at TIMESTAMP WITH TIME ZONE,
    finished_at TIMESTAMP WITH TIME ZONE,
    deleted_at TIMESTAMP WITH TIME ZONE,
    archived_at TIMESTAMP WITH TIME ZONE,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT now() NOT NULL,
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT now() NOT NULL,
    CONSTRAINT pk_job_items PRIMARY KEY (id),
    CONSTRAINT uq_job_items_job_id_item_index UNIQUE (job_id, item_index),
    CONSTRAINT uq_job_items_job_id_id UNIQUE (job_id, id),
    CONSTRAINT fk_job_items_job_id_jobs FOREIGN KEY (job_id) REFERENCES jobs (id),
    CONSTRAINT ck_job_items_job_item_status CHECK (status IN ('pending', 'ready', 'running', 'succeeded', 'failed_business', 'failed_system', 'quota_skipped', 'cancelled_before_start', 'cancelled_after_start'))
);

CREATE TABLE IF NOT EXISTS ai_executions (
    id BIGSERIAL NOT NULL,
    job_id BIGINT NOT NULL,
    job_item_id BIGINT NOT NULL,
    user_id BIGINT NOT NULL,
    status VARCHAR(21) NOT NULL,
    provider VARCHAR(64) NOT NULL,
    model VARCHAR(128) NOT NULL,
    request_idempotency_key VARCHAR(128),
    workflow_version VARCHAR(32) NOT NULL,
    step_protocol_version VARCHAR(32) NOT NULL,
    result_schema_version VARCHAR(32) NOT NULL,
    step_type VARCHAR(64) NOT NULL,
    step_id VARCHAR(128) NOT NULL,
    input_summary TEXT NOT NULL,
    input_payload JSONB DEFAULT '{}'::jsonb NOT NULL,
    result_summary TEXT NOT NULL,
    result_payload JSONB DEFAULT '{}'::jsonb NOT NULL,
    error_summary TEXT NOT NULL,
    error_payload JSONB DEFAULT '{}'::jsonb NOT NULL,
    started_at TIMESTAMP WITH TIME ZONE,
    finished_at TIMESTAMP WITH TIME ZONE,
    deleted_at TIMESTAMP WITH TIME ZONE,
    archived_at TIMESTAMP WITH TIME ZONE,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT now() NOT NULL,
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT now() NOT NULL,
    CONSTRAINT pk_ai_executions PRIMARY KEY (id),
    CONSTRAINT fk_ai_executions_job_id_jobs FOREIGN KEY (job_id) REFERENCES jobs (id),
    CONSTRAINT fk_ai_executions_job_item_chain FOREIGN KEY (job_id, job_item_id) REFERENCES job_items (job_id, id),
    CONSTRAINT ck_ai_executions_ai_execution_status CHECK (status IN ('created', 'dispatching', 'running', 'succeeded', 'failed_business', 'failed_system', 'completed_with_refund'))
);

CREATE TABLE IF NOT EXISTS execution_charges (
    id BIGSERIAL NOT NULL,
    ai_execution_id BIGINT NOT NULL,
    user_id BIGINT NOT NULL,
    charge_status VARCHAR(8) NOT NULL,
    charge_unit VARCHAR(32) NOT NULL,
    charge_amount INTEGER NOT NULL,
    refund_reason TEXT NOT NULL,
    reserved_at TIMESTAMP WITH TIME ZONE,
    captured_at TIMESTAMP WITH TIME ZONE,
    refunded_at TIMESTAMP WITH TIME ZONE,
    deleted_at TIMESTAMP WITH TIME ZONE,
    archived_at TIMESTAMP WITH TIME ZONE,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT now() NOT NULL,
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT now() NOT NULL,
    CONSTRAINT pk_execution_charges PRIMARY KEY (id),
    CONSTRAINT uq_execution_charges_ai_execution_id UNIQUE (ai_execution_id),
    CONSTRAINT fk_execution_charges_ai_execution_id_ai_executions FOREIGN KEY (ai_execution_id) REFERENCES ai_executions (id),
    CONSTRAINT ck_execution_charges_charge_status CHECK (charge_status IN ('reserved', 'captured', 'refunded'))
);

CREATE TABLE IF NOT EXISTS quota_buckets (
    id BIGSERIAL NOT NULL,
    quota_account_id BIGINT NOT NULL,
    bucket_type VARCHAR(6) NOT NULL,
    period_start TIMESTAMP WITH TIME ZONE NOT NULL,
    period_end TIMESTAMP WITH TIME ZONE NOT NULL,
    quota_limit INTEGER NOT NULL,
    used_amount INTEGER NOT NULL,
    refunded_amount INTEGER NOT NULL,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT now() NOT NULL,
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT now() NOT NULL,
    CONSTRAINT pk_quota_buckets PRIMARY KEY (id),
    CONSTRAINT ck_quota_buckets_quota_buckets_period CHECK (period_end > period_start),
    CONSTRAINT uq_quota_buckets_period UNIQUE (quota_account_id, bucket_type, period_start, period_end),
    CONSTRAINT fk_quota_buckets_quota_account_id_quota_accounts FOREIGN KEY (quota_account_id) REFERENCES quota_accounts (id),
    CONSTRAINT ck_quota_buckets_quota_bucket_type CHECK (bucket_type IN ('daily', 'weekly'))
);

CREATE TABLE IF NOT EXISTS usage_ledgers (
    id BIGSERIAL NOT NULL,
    user_id BIGINT NOT NULL,
    quota_account_id BIGINT NOT NULL,
    quota_bucket_id BIGINT,
    ai_execution_id BIGINT,
    ledger_type VARCHAR(7) NOT NULL,
    amount INTEGER NOT NULL,
    balance_after INTEGER NOT NULL,
    reason_code VARCHAR(64) NOT NULL,
    deleted_at TIMESTAMP WITH TIME ZONE,
    archived_at TIMESTAMP WITH TIME ZONE,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT now() NOT NULL,
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT now() NOT NULL,
    CONSTRAINT pk_usage_ledgers PRIMARY KEY (id),
    CONSTRAINT fk_usage_ledgers_quota_account_id_quota_accounts FOREIGN KEY (quota_account_id) REFERENCES quota_accounts (id),
    CONSTRAINT fk_usage_ledgers_quota_bucket_id_quota_buckets FOREIGN KEY (quota_bucket_id) REFERENCES quota_buckets (id),
    CONSTRAINT fk_usage_ledgers_ai_execution_id_ai_executions FOREIGN KEY (ai_execution_id) REFERENCES ai_executions (id),
    CONSTRAINT ck_usage_ledgers_ledger_type CHECK (ledger_type IN ('reserve', 'capture', 'refund'))
);

CREATE TABLE IF NOT EXISTS artifacts (
    id BIGSERIAL NOT NULL,
    job_id BIGINT NOT NULL,
    job_item_id BIGINT,
    ai_execution_id BIGINT,
    user_id BIGINT NOT NULL,
    artifact_type VARCHAR(64) NOT NULL,
    storage_provider VARCHAR(64) NOT NULL,
    object_key VARCHAR(256) NOT NULL,
    file_name VARCHAR(256),
    mime_type VARCHAR(128),
    size_bytes BIGINT,
    result_summary TEXT NOT NULL,
    deleted_at TIMESTAMP WITH TIME ZONE,
    file_cleanup_requested_at TIMESTAMP WITH TIME ZONE,
    file_cleaned_at TIMESTAMP WITH TIME ZONE,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT now() NOT NULL,
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT now() NOT NULL,
    CONSTRAINT pk_artifacts PRIMARY KEY (id),
    CONSTRAINT fk_artifacts_job_id_jobs FOREIGN KEY (job_id) REFERENCES jobs (id),
    CONSTRAINT fk_artifacts_job_item_chain FOREIGN KEY (job_item_id, job_id) REFERENCES job_items (id, job_id),
    CONSTRAINT fk_artifacts_ai_execution_id_ai_executions FOREIGN KEY (ai_execution_id) REFERENCES ai_executions (id)
);

CREATE TABLE IF NOT EXISTS job_events (
    id BIGSERIAL NOT NULL,
    job_id BIGINT NOT NULL,
    job_item_id BIGINT,
    ai_execution_id BIGINT,
    user_id BIGINT NOT NULL,
    event_type VARCHAR(128) NOT NULL,
    event_payload JSONB DEFAULT '{}'::jsonb NOT NULL,
    deleted_at TIMESTAMP WITH TIME ZONE,
    archived_at TIMESTAMP WITH TIME ZONE,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT now() NOT NULL,
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT now() NOT NULL,
    CONSTRAINT pk_job_events PRIMARY KEY (id),
    CONSTRAINT fk_job_events_job_id_jobs FOREIGN KEY (job_id) REFERENCES jobs (id),
    CONSTRAINT fk_job_events_job_item_chain FOREIGN KEY (job_item_id, job_id) REFERENCES job_items (id, job_id),
    CONSTRAINT fk_job_events_ai_execution_id_ai_executions FOREIGN KEY (ai_execution_id) REFERENCES ai_executions (id)
);

CREATE INDEX IF NOT EXISTS ix_jobs_user_id_created_at_desc
    ON jobs (user_id, created_at DESC);

CREATE INDEX IF NOT EXISTS ix_jobs_status_created_at_desc
    ON jobs (status, created_at DESC);

CREATE INDEX IF NOT EXISTS ix_job_items_job_id_item_index
    ON job_items (job_id, item_index);

CREATE INDEX IF NOT EXISTS ix_ai_executions_job_item_id_created_at
    ON ai_executions (job_item_id, created_at);

CREATE UNIQUE INDEX IF NOT EXISTS uq_ai_executions_user_item_idempotency_key
    ON ai_executions (user_id, job_item_id, request_idempotency_key)
    WHERE request_idempotency_key IS NOT NULL;

CREATE INDEX IF NOT EXISTS ix_usage_ledgers_user_id_created_at_desc
    ON usage_ledgers (user_id, created_at DESC);

CREATE INDEX IF NOT EXISTS ix_artifacts_job_item_id_created_at
    ON artifacts (job_item_id, created_at);

CREATE INDEX IF NOT EXISTS ix_job_events_job_id_created_at
    ON job_events (job_id, created_at);

COMMIT;
