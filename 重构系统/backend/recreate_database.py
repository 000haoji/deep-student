"""
重新创建数据库脚本
- 如果是SQLite，则删除旧数据库文件（基于settings.DATABASE_URL中的文件名，并假设其位于backend/目录）。
- 如果是PostgreSQL，则先清空所有表。
- 然后根据当前模型创建所有表。
- 尝试创建演示用户。
"""
import asyncio
import os
from urllib.parse import urlparse
import sys
from pathlib import Path

# Add project root to sys.path to allow imports like shared.database
# Assumes this script is in backend/
sys.path.insert(0, str(Path(__file__).resolve().parent.parent))

from shared.database import engine, Base  # Import engine and Base
from shared.config import settings      # Import settings
from shared.utils.logger import get_logger

logger = get_logger(__name__)

async def drop_all_tables():
    """Drops all tables defined in Base.metadata."""
    try:
        async with engine.begin() as conn:
            logger.info("Dropping all existing tables...")
            await conn.run_sync(Base.metadata.drop_all)
            logger.info("All tables dropped successfully.")
    except Exception as e:
        logger.error(f"Failed to drop tables: {e}")
        # Depending on the DB, drop_all might fail if tables don't exist or due to constraints.
        # It's often fine to proceed to create_all, which might also handle non-existence.

async def create_all_tables():
    """Creates all tables defined in Base.metadata."""
    try:
        # Ensure all SQLAlchemy models are imported before calling create_all
        # so that Base.metadata is populated.
        # This is crucial.
        import services.user_service.models
        import services.problem_service.models
        import services.ai_api_manager.models
        import services.review_service.models
        import services.file_service.models
        # Add any other service model modules here

        async with engine.begin() as conn:
            logger.info("Creating all tables based on current SQLAlchemy models...")
            await conn.run_sync(Base.metadata.create_all)
            logger.info("All tables created successfully.")
    except Exception as e:
        logger.error(f"Failed to create tables: {e}")
        raise

async def recreate_database_logic():
    """
    Recreates the database.
    If SQLite, deletes the DB file first.
    Then drops all tables (for PostgreSQL) and recreates them based on models.
    """
    logger.info("Starting database recreation process...")
    logger.info(f"Using DATABASE_URL: {settings.DATABASE_URL}")

    parsed_url = urlparse(settings.DATABASE_URL)

    if parsed_url.scheme.startswith("sqlite"):
        # For SQLite, path is like '/path/to/db.sqlite' or '/./db.sqlite' or 'db.sqlite'
        # We need to derive the actual file path.
        # os.path.basename will give 'db.sqlite'
        # Let's assume the SQLite file is in the 'backend' directory, next to this script.
        sqlite_filename = os.path.basename(parsed_url.path)
        if not sqlite_filename: # Handles cases like "sqlite+aiosqlite:///:memory:" or if path is just "/"
            logger.warning("SQLite URL does not specify a filename. Cannot delete. Assuming in-memory or to be created.")
        else:
            # Assumes the file is in the same directory as this script (backend/)
            db_file_in_backend_dir = Path(__file__).parent / sqlite_filename
            
            logger.info(f"Detected SQLite database. Target file: {db_file_in_backend_dir}")
            if db_file_in_backend_dir.exists():
                try:
                    os.remove(db_file_in_backend_dir)
                    logger.info(f"Deleted old SQLite database file: {db_file_in_backend_dir}")
                except OSError as e:
                    logger.error(f"Error deleting SQLite file {db_file_in_backend_dir}: {e}. Proceeding to table creation.")
            else:
                logger.info(f"SQLite database file not found at {db_file_in_backend_dir}. A new one will be created if specified by DATABASE_URL.")
        
        # For SQLite, create_all will create the file if it doesn't exist.
        # No need to drop tables if the file is deleted.
        await create_all_tables()

    elif parsed_url.scheme.startswith("postgresql"):
        logger.info("Detected PostgreSQL database. Will drop and recreate all tables.")
        # For PostgreSQL (and other server-based DBs), drop tables then create them.
        await drop_all_tables()
        await create_all_tables()
    else:
        logger.warning(f"Unsupported database scheme: {parsed_url.scheme}. Will attempt to create tables only.")
        # Fallback: just try to create tables, might fail if they exist.
        await create_all_tables()

    # Attempt to create a demo user
    try:
        # Assuming init_db_simple.py is in the same directory (backend/)
        from init_db_simple import create_demo_user as create_demo_user_func
        logger.info("Attempting to create demo user...")
        await create_demo_user_func() # Call the async function
        logger.info("Demo user creation process finished (check logs from create_demo_user for success/failure).")
    except ImportError:
        logger.warning("Could not import 'create_demo_user' from 'init_db_simple.py'. Skipping demo user creation.")
    except Exception as e:
        logger.error(f"An error occurred while trying to create demo user: {e}")

    logger.info("✅ Database recreation process finished.")
    logger.info("Please check logs for any errors during table creation or demo user setup.")


if __name__ == "__main__":
    asyncio.run(recreate_database_logic())
