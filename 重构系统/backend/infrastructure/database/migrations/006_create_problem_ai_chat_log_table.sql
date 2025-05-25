-- 错题AI交互日志表
CREATE TABLE IF NOT EXISTS problem_ai_chat_logs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    problem_id UUID NOT NULL,
    role VARCHAR(20) NOT NULL, -- 'user', 'ai', 'system'
    content TEXT NOT NULL,
    content_type VARCHAR(50) DEFAULT 'text', -- 'text', 'json_suggestion', 'error_message', etc.
    order_in_conversation INTEGER NOT NULL,
    
    -- 时间戳
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,

    CONSTRAINT fk_problem_ai_chat_logs_problem_id FOREIGN KEY (problem_id) REFERENCES problems(id) ON DELETE CASCADE
);

-- 说明:
-- problem_id: 关联到 problems 表的错题ID。ON DELETE CASCADE 确保当错题被删除时，相关的聊天记录也会被删除。
-- role: 消息发送者的角色，例如 'user' (用户), 'ai' (AI助手), 'system' (系统消息，如初始上下文提示)。
-- content: 消息的具体内容。
-- content_type: 内容的类型，默认为 'text'。可以扩展为 'json_suggestion' (AI提出的结构化数据修改建议), 'error_message' (流处理中的错误) 等。
-- order_in_conversation: 消息在对话中的顺序，用于重建对话历史。

-- 索引
CREATE INDEX IF NOT EXISTS idx_problem_ai_chat_logs_problem_id ON problem_ai_chat_logs(problem_id);
CREATE INDEX IF NOT EXISTS idx_problem_ai_chat_logs_created_at ON problem_ai_chat_logs(created_at);
-- 复合索引，用于高效查询特定错题的对话历史并按顺序排序
CREATE INDEX IF NOT EXISTS idx_problem_ai_chat_logs_order ON problem_ai_chat_logs(problem_id, order_in_conversation);

-- 自动更新 updated_at 时间戳的触发器函数 (如果项目中统一使用此模式)
-- 注意: SQLBaseModel 可能已经通过 SQLAlchemy 的事件处理了 updated_at 的更新。
-- 如果没有，并且需要数据库层面处理，则可以使用以下触发器。
-- 首先检查项目中是否已有类似的通用触发器函数。

-- CREATE OR REPLACE FUNCTION update_updated_at_column()
-- RETURNS TRIGGER AS $$
-- BEGIN
--     NEW.updated_at = CURRENT_TIMESTAMP;
--     RETURN NEW;
-- END;
-- $$ language 'plpgsql';

-- 如果需要为 problem_ai_chat_logs 表单独创建触发器:
-- CREATE TRIGGER trigger_problem_ai_chat_logs_updated_at
-- BEFORE UPDATE ON problem_ai_chat_logs
-- FOR EACH ROW
-- EXECUTE FUNCTION update_updated_at_column();

COMMENT ON TABLE problem_ai_chat_logs IS '存储错题与其相关的AI助手交互的聊天记录。';
COMMENT ON COLUMN problem_ai_chat_logs.problem_id IS '关联的错题ID (外键指向 problems.id)。';
COMMENT ON COLUMN problem_ai_chat_logs.role IS '消息发送者的角色 (e.g., user, ai, system)。';
COMMENT ON COLUMN problem_ai_chat_logs.content IS '消息的文本内容。';
COMMENT ON COLUMN problem_ai_chat_logs.content_type IS '消息内容的类型 (e.g., text, json_suggestion)。';
COMMENT ON COLUMN problem_ai_chat_logs.order_in_conversation IS '消息在特定错题对话中的顺序。';
