"""
修复数据库 - 添加缺失的 deleted_at 列
"""
import sqlite3
import os

def fix_database():
    """修复数据库表结构"""
    db_file = "error_management.db"
    
    if not os.path.exists(db_file):
        print(f"数据库文件 {db_file} 不存在")
        return
    
    conn = sqlite3.connect(db_file)
    cursor = conn.cursor()
    
    try:
        # 添加 deleted_at 列到 problems 表
        cursor.execute("ALTER TABLE problems ADD COLUMN deleted_at DATETIME")
        conn.commit()
        print("成功添加 deleted_at 列到 problems 表")
    except sqlite3.OperationalError as e:
        if "duplicate column name" in str(e):
            print("deleted_at 列已存在")
        else:
            print(f"错误: {e}")
    finally:
        conn.close()

if __name__ == "__main__":
    fix_database() 