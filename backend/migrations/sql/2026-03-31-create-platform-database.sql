-- 平台后端数据库初始化 SQL
-- 默认数据库名：agent_the_spire_platform
-- 执行前请使用具有 CREATEDB 权限的 PostgreSQL 管理账号连接到 postgres 或其他管理库。

CREATE DATABASE agent_the_spire_platform
    WITH
    OWNER = postgres
    ENCODING = 'UTF8'
    TEMPLATE = template0;
