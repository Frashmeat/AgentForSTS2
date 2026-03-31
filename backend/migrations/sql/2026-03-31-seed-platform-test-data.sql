-- 平台后端最小联调测试数据
-- 注意：本文件会清空平台主链相关表，请勿在生产环境执行。
-- 执行前请先完成 2026-03-31-create-platform-database.sql。

\set ON_ERROR_STOP on
\connect agent_the_spire_platform

BEGIN;

TRUNCATE TABLE
    job_events,
    artifacts,
    usage_ledgers,
    execution_charges,
    ai_executions,
    job_items,
    quota_buckets,
    jobs,
    quota_accounts
RESTART IDENTITY CASCADE;

INSERT INTO quota_accounts (
    id,
    user_id,
    status,
    created_at,
    updated_at
) VALUES (
    1001,
    1001,
    'active',
    TIMESTAMPTZ '2026-03-31 09:00:00+00',
    TIMESTAMPTZ '2026-03-31 09:00:00+00'
);

INSERT INTO quota_buckets (
    id,
    quota_account_id,
    bucket_type,
    period_start,
    period_end,
    quota_limit,
    used_amount,
    refunded_amount,
    created_at,
    updated_at
) VALUES (
    1101,
    1001,
    'daily',
    TIMESTAMPTZ '2026-03-31 00:00:00+00',
    TIMESTAMPTZ '2026-04-01 00:00:00+00',
    10,
    2,
    1,
    TIMESTAMPTZ '2026-03-31 09:00:00+00',
    TIMESTAMPTZ '2026-03-31 09:20:00+00'
);

INSERT INTO jobs (
    id,
    user_id,
    job_type,
    status,
    workflow_version,
    input_summary,
    result_summary,
    error_summary,
    total_item_count,
    pending_item_count,
    running_item_count,
    succeeded_item_count,
    failed_business_item_count,
    failed_system_item_count,
    quota_skipped_item_count,
    cancelled_before_start_item_count,
    cancelled_after_start_item_count,
    started_at,
    finished_at,
    cancel_requested_at,
    deleted_at,
    archived_at,
    created_at,
    updated_at
) VALUES (
    2001,
    1001,
    'batch_generate',
    'partial_succeeded',
    '2026.03.31',
    '为测试用户生成两张平台资产',
    '1 个子项成功，1 个子项系统失败并退款',
    '',
    2,
    0,
    0,
    1,
    0,
    1,
    0,
    0,
    0,
    TIMESTAMPTZ '2026-03-31 09:05:00+00',
    TIMESTAMPTZ '2026-03-31 09:20:00+00',
    NULL,
    NULL,
    NULL,
    TIMESTAMPTZ '2026-03-31 09:00:00+00',
    TIMESTAMPTZ '2026-03-31 09:20:00+00'
);

INSERT INTO job_items (
    id,
    job_id,
    user_id,
    item_index,
    item_type,
    status,
    input_summary,
    input_payload,
    result_summary,
    error_summary,
    attempt_count,
    started_at,
    finished_at,
    deleted_at,
    archived_at,
    created_at,
    updated_at
) VALUES
(
    2101,
    2001,
    1001,
    0,
    'card',
    'succeeded',
    '生成测试卡牌 A',
    '{"name":"TestCardA","style":"platform"}'::jsonb,
    '生成 PNG 资源并完成代码写入',
    '',
    1,
    TIMESTAMPTZ '2026-03-31 09:06:00+00',
    TIMESTAMPTZ '2026-03-31 09:12:00+00',
    NULL,
    NULL,
    TIMESTAMPTZ '2026-03-31 09:00:30+00',
    TIMESTAMPTZ '2026-03-31 09:12:00+00'
),
(
    2102,
    2001,
    1001,
    1,
    'relic',
    'failed_system',
    '生成测试遗物 B',
    '{"name":"TestRelicB","style":"platform"}'::jsonb,
    '',
    '上游模型超时，已触发退款',
    1,
    TIMESTAMPTZ '2026-03-31 09:13:00+00',
    TIMESTAMPTZ '2026-03-31 09:18:00+00',
    NULL,
    NULL,
    TIMESTAMPTZ '2026-03-31 09:00:30+00',
    TIMESTAMPTZ '2026-03-31 09:18:00+00'
);

INSERT INTO ai_executions (
    id,
    job_id,
    job_item_id,
    user_id,
    status,
    provider,
    model,
    request_idempotency_key,
    workflow_version,
    step_protocol_version,
    result_schema_version,
    step_type,
    step_id,
    input_summary,
    input_payload,
    result_summary,
    result_payload,
    error_summary,
    error_payload,
    started_at,
    finished_at,
    deleted_at,
    archived_at,
    created_at,
    updated_at
) VALUES
(
    2201,
    2001,
    2101,
    1001,
    'succeeded',
    'openai',
    'gpt-5.4',
    'idem-success-2101',
    '2026.03.31',
    'v1',
    'v1',
    'image.generate',
    'step-card-001',
    '根据描述生成测试卡牌 A',
    '{"prompt":"generate card A"}'::jsonb,
    '图片与元数据已返回',
    '{"artifact_key":"artifacts/test-card-a.png"}'::jsonb,
    '',
    '{}'::jsonb,
    TIMESTAMPTZ '2026-03-31 09:06:10+00',
    TIMESTAMPTZ '2026-03-31 09:10:00+00',
    NULL,
    NULL,
    TIMESTAMPTZ '2026-03-31 09:06:10+00',
    TIMESTAMPTZ '2026-03-31 09:10:00+00'
),
(
    2202,
    2001,
    2102,
    1001,
    'completed_with_refund',
    'openai',
    'gpt-5.4',
    'idem-refund-2102',
    '2026.03.31',
    'v1',
    'v1',
    'image.generate',
    'step-relic-001',
    '根据描述生成测试遗物 B',
    '{"prompt":"generate relic B"}'::jsonb,
    '',
    '{}'::jsonb,
    'provider timeout',
    '{"code":"provider_timeout"}'::jsonb,
    TIMESTAMPTZ '2026-03-31 09:13:05+00',
    TIMESTAMPTZ '2026-03-31 09:17:30+00',
    NULL,
    NULL,
    TIMESTAMPTZ '2026-03-31 09:13:05+00',
    TIMESTAMPTZ '2026-03-31 09:17:30+00'
);

INSERT INTO execution_charges (
    id,
    ai_execution_id,
    user_id,
    charge_status,
    charge_unit,
    charge_amount,
    refund_reason,
    reserved_at,
    captured_at,
    refunded_at,
    deleted_at,
    archived_at,
    created_at,
    updated_at
) VALUES
(
    2301,
    2201,
    1001,
    'captured',
    'execution',
    1,
    '',
    TIMESTAMPTZ '2026-03-31 09:06:10+00',
    TIMESTAMPTZ '2026-03-31 09:10:00+00',
    NULL,
    NULL,
    NULL,
    TIMESTAMPTZ '2026-03-31 09:06:10+00',
    TIMESTAMPTZ '2026-03-31 09:10:00+00'
),
(
    2302,
    2202,
    1001,
    'refunded',
    'execution',
    1,
    'system_error',
    TIMESTAMPTZ '2026-03-31 09:13:05+00',
    TIMESTAMPTZ '2026-03-31 09:15:00+00',
    TIMESTAMPTZ '2026-03-31 09:17:45+00',
    NULL,
    NULL,
    TIMESTAMPTZ '2026-03-31 09:13:05+00',
    TIMESTAMPTZ '2026-03-31 09:17:45+00'
);

INSERT INTO usage_ledgers (
    id,
    user_id,
    quota_account_id,
    quota_bucket_id,
    ai_execution_id,
    ledger_type,
    amount,
    balance_after,
    reason_code,
    deleted_at,
    archived_at,
    created_at,
    updated_at
) VALUES
(
    2401,
    1001,
    1001,
    1101,
    2201,
    'reserve',
    1,
    9,
    'execution_reserved',
    NULL,
    NULL,
    TIMESTAMPTZ '2026-03-31 09:06:10+00',
    TIMESTAMPTZ '2026-03-31 09:06:10+00'
),
(
    2402,
    1001,
    1001,
    1101,
    2201,
    'capture',
    0,
    9,
    'execution_captured',
    NULL,
    NULL,
    TIMESTAMPTZ '2026-03-31 09:10:00+00',
    TIMESTAMPTZ '2026-03-31 09:10:00+00'
),
(
    2403,
    1001,
    1001,
    1101,
    2202,
    'reserve',
    1,
    8,
    'execution_reserved',
    NULL,
    NULL,
    TIMESTAMPTZ '2026-03-31 09:13:05+00',
    TIMESTAMPTZ '2026-03-31 09:13:05+00'
),
(
    2404,
    1001,
    1001,
    1101,
    2202,
    'refund',
    1,
    9,
    'system_error_refund',
    NULL,
    NULL,
    TIMESTAMPTZ '2026-03-31 09:17:45+00',
    TIMESTAMPTZ '2026-03-31 09:17:45+00'
);

INSERT INTO artifacts (
    id,
    job_id,
    job_item_id,
    ai_execution_id,
    user_id,
    artifact_type,
    storage_provider,
    object_key,
    file_name,
    mime_type,
    size_bytes,
    result_summary,
    deleted_at,
    file_cleanup_requested_at,
    file_cleaned_at,
    created_at,
    updated_at
) VALUES (
    2501,
    2001,
    2101,
    2201,
    1001,
    'image',
    'local_fs',
    'artifacts/test-card-a.png',
    'test-card-a.png',
    'image/png',
    4096,
    '成功生成测试图片产物',
    NULL,
    NULL,
    NULL,
    TIMESTAMPTZ '2026-03-31 09:10:05+00',
    TIMESTAMPTZ '2026-03-31 09:10:05+00'
);

INSERT INTO job_events (
    id,
    job_id,
    job_item_id,
    ai_execution_id,
    user_id,
    event_type,
    event_payload,
    deleted_at,
    archived_at,
    created_at,
    updated_at
) VALUES
(
    2601,
    2001,
    NULL,
    NULL,
    1001,
    'job.created',
    '{"status":"draft"}'::jsonb,
    NULL,
    NULL,
    TIMESTAMPTZ '2026-03-31 09:00:00+00',
    TIMESTAMPTZ '2026-03-31 09:00:00+00'
),
(
    2602,
    2001,
    NULL,
    NULL,
    1001,
    'job.queued',
    '{"status":"queued"}'::jsonb,
    NULL,
    NULL,
    TIMESTAMPTZ '2026-03-31 09:05:00+00',
    TIMESTAMPTZ '2026-03-31 09:05:00+00'
),
(
    2603,
    2001,
    2101,
    2201,
    1001,
    'ai_execution.finished',
    '{"status":"succeeded","artifact_id":2501}'::jsonb,
    NULL,
    NULL,
    TIMESTAMPTZ '2026-03-31 09:10:05+00',
    TIMESTAMPTZ '2026-03-31 09:10:05+00'
),
(
    2604,
    2001,
    2102,
    2202,
    1001,
    'ai_execution.refunded',
    '{"status":"completed_with_refund","refund_reason":"system_error"}'::jsonb,
    NULL,
    NULL,
    TIMESTAMPTZ '2026-03-31 09:17:45+00',
    TIMESTAMPTZ '2026-03-31 09:17:45+00'
);

SELECT setval(pg_get_serial_sequence('quota_accounts', 'id'), COALESCE((SELECT MAX(id) FROM quota_accounts), 1), true);
SELECT setval(pg_get_serial_sequence('quota_buckets', 'id'), COALESCE((SELECT MAX(id) FROM quota_buckets), 1), true);
SELECT setval(pg_get_serial_sequence('jobs', 'id'), COALESCE((SELECT MAX(id) FROM jobs), 1), true);
SELECT setval(pg_get_serial_sequence('job_items', 'id'), COALESCE((SELECT MAX(id) FROM job_items), 1), true);
SELECT setval(pg_get_serial_sequence('ai_executions', 'id'), COALESCE((SELECT MAX(id) FROM ai_executions), 1), true);
SELECT setval(pg_get_serial_sequence('execution_charges', 'id'), COALESCE((SELECT MAX(id) FROM execution_charges), 1), true);
SELECT setval(pg_get_serial_sequence('usage_ledgers', 'id'), COALESCE((SELECT MAX(id) FROM usage_ledgers), 1), true);
SELECT setval(pg_get_serial_sequence('artifacts', 'id'), COALESCE((SELECT MAX(id) FROM artifacts), 1), true);
SELECT setval(pg_get_serial_sequence('job_events', 'id'), COALESCE((SELECT MAX(id) FROM job_events), 1), true);

COMMIT;
