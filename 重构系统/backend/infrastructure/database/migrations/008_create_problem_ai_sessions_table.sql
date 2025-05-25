-- 008_create_problem_ai_sessions_table.sql
-- Description: Creates the problem_ai_sessions table and updates problem_ai_chat_logs to link to it.

-- Create the problem_ai_sessions table
CREATE TABLE problem_ai_sessions (
    id VARCHAR(36) NOT NULL,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP,
    status VARCHAR(50) NOT NULL DEFAULT 'active', -- Corresponds to ProblemAISessionStatus enum (e.g., 'active', 'finalized', 'aborted')
    current_structured_data JSONB,
    final_problem_id VARCHAR(36),
    initial_image_ref VARCHAR(1024),
    initial_subject_hint VARCHAR(50), -- Corresponds to Subject enum (e.g., 'math', 'english')
    
    PRIMARY KEY (id),
    CONSTRAINT fk_problem_ai_sessions_final_problem FOREIGN KEY(final_problem_id) REFERENCES problems(id) ON DELETE SET NULL
);

-- Add index for faster lookups on final_problem_id
CREATE INDEX IF NOT EXISTS idx_problem_ai_sessions_final_problem_id ON problem_ai_sessions(final_problem_id);

-- Update problem_ai_chat_logs table to add foreign key constraint
-- Assumption: problem_ai_chat_logs.problem_creation_session_id column type is already VARCHAR(36) or compatible.
-- If existing data in problem_creation_session_id might violate the FK, those rows/sessions would need handling.
-- For a new setup or controlled environment, this should be fine.
-- The ON DELETE SET NULL clause means if a session is deleted, chat logs will have their session ID nulled but logs remain.
ALTER TABLE problem_ai_chat_logs
ADD CONSTRAINT fk_problem_ai_chat_logs_ai_session
FOREIGN KEY (problem_creation_session_id)
REFERENCES problem_ai_sessions(id)
ON DELETE SET NULL;

-- The index on problem_ai_chat_logs.problem_creation_session_id was likely created in migration 007.
-- If not, it should be added:
-- CREATE INDEX IF NOT EXISTS idx_problem_ai_chat_logs_problem_creation_session_id ON problem_ai_chat_logs(problem_creation_session_id);

COMMENT ON TABLE problem_ai_sessions IS 'Stores AI-driven problem creation sessions.';
COMMENT ON COLUMN problem_ai_sessions.id IS 'Unique identifier for the AI creation session (e.g., a UUID string).';
COMMENT ON COLUMN problem_ai_sessions.status IS 'Current status of the AI session (active, finalized, aborted).';
COMMENT ON COLUMN problem_ai_sessions.current_structured_data IS 'JSON blob storing the evolving structured data of the problem being created.';
COMMENT ON COLUMN problem_ai_sessions.final_problem_id IS 'ForeignKey to the problems table, linking to the finally created problem.';
COMMENT ON COLUMN problem_ai_sessions.initial_image_ref IS 'Reference (e.g., URL) to the initial image uploaded for the session.';
COMMENT ON COLUMN problem_ai_sessions.initial_subject_hint IS 'Subject hint provided by the user at the start of the session.';

COMMENT ON CONSTRAINT fk_problem_ai_chat_logs_ai_session ON problem_ai_chat_logs IS 'Links chat logs to their corresponding AI creation session.';
