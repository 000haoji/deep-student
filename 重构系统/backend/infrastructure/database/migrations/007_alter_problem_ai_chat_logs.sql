-- 迁移描述:
-- 1. 为 problem_ai_chat_logs 表添加 problem_creation_session_id 字段，用于在AI创建流程中关联聊天记录。
-- 2. 将 problem_ai_chat_logs 表的 problem_id 字段修改为允许 NULL，因为在AI创建会话期间，problem_id 可能尚未确定。

-- 添加 problem_creation_session_id 字段
ALTER TABLE problem_ai_chat_logs
ADD COLUMN IF NOT EXISTS problem_creation_session_id VARCHAR(255);

-- 为 problem_creation_session_id 添加索引 (如果不存在)
CREATE INDEX IF NOT EXISTS idx_problem_ai_chat_logs_session_id ON problem_ai_chat_logs(problem_creation_session_id);

-- 修改 problem_id 字段为允许 NULL
-- 注意: 不同数据库修改列的可空性的语法可能不同。以下为 PostgreSQL 示例。
-- 对于 SQLite, ALTER TABLE ... ALTER COLUMN ... SET NULL 不直接支持。
-- 通常需要重命名表，创建新表，然后复制数据。
-- 假设目标是 PostgreSQL。
-- 检查列是否已为 NULLABLE，避免重复执行导致错误 (某些数据库系统可能需要此检查)
DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_name = 'problem_ai_chat_logs'
        AND column_name = 'problem_id'
        AND is_nullable = 'NO'
    ) THEN
        ALTER TABLE problem_ai_chat_logs
        ALTER COLUMN problem_id DROP NOT NULL;
    END IF;
END $$;

COMMENT ON COLUMN problem_ai_chat_logs.problem_id IS '关联的错题ID (外键指向 problems.id)。在AI创建会话期间可为空。';
COMMENT ON COLUMN problem_ai_chat_logs.problem_creation_session_id IS 'AI错题创建流程中使用的会话ID，用于在错题最终确定前关联日志。';

-- 注意: 如果之前 problem_id 的外键约束是 ON DELETE CASCADE NOT NULL，
-- 并且现在 problem_id 变为 NULLABLE，其行为（例如父记录删除时）可能需要审视。
-- 当前模型定义为 ON DELETE CASCADE，如果 problem_id 为 NULL，则此级联不适用。
-- 如果 problem_id 非 NULL，则级联删除行为保持。
