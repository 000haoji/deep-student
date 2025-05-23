"""
检查数据库表结构
"""
import sqlite3

def check_database():
    """检查数据库表结构"""
    conn = sqlite3.connect("error_management.db")
    cursor = conn.cursor()
    
    # 获取所有表
    cursor.execute("SELECT name FROM sqlite_master WHERE type='table';")
    tables = cursor.fetchall()
    print("数据库中的表：")
    for table in tables:
        print(f"- {table[0]}")
    
    # 检查 problems 表的结构
    print("\nproblems 表结构：")
    cursor.execute("PRAGMA table_info(problems);")
    columns = cursor.fetchall()
    for col in columns:
        print(f"  {col[1]} {col[2]}")
    
    conn.close()

if __name__ == "__main__":
    check_database() 