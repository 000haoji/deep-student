-- Migration 009: Update ai_models table schema
-- Add new columns
ALTER TABLE ai_models ADD COLUMN api_key_env_var VARCHAR(255) NULL;
ALTER TABLE ai_models ADD COLUMN base_url_env_var VARCHAR(255) NULL;
ALTER TABLE ai_models ADD COLUMN supported_task_types TEXT NULL; -- For JSON
ALTER TABLE ai_models ADD COLUMN cost_per_1k_input_tokens REAL NULL;
ALTER TABLE ai_models ADD COLUMN cost_per_1k_output_tokens REAL NULL;
ALTER TABLE ai_models ADD COLUMN rpm_limit INTEGER NULL;
ALTER TABLE ai_models ADD COLUMN tpm_limit INTEGER NULL;

-- Rename existing columns to match current model attribute names
-- These renames assume the old columns 'max_tokens' and 'timeout' exist from the initial migration
-- and the new columns 'max_tokens_limit' and 'timeout_seconds' do not already exist.
ALTER TABLE ai_models RENAME COLUMN max_tokens TO max_tokens_limit;
ALTER TABLE ai_models RENAME COLUMN timeout TO timeout_seconds;

-- Note: The default value for timeout_seconds (formerly timeout) changed from 30 to 120 in the model.
-- This rename operation does not update existing values or the column's default constraint in the database.
-- New rows will receive defaults from the SQLAlchemy model definition if the database column allows NULL
-- or if the database default is not overriding.
