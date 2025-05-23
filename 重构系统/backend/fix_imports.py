"""
批量修复导入路径脚本
"""
import os
import re
from pathlib import Path

def fix_imports_in_file(filepath):
    """修复单个文件中的导入路径"""
    with open(filepath, 'r', encoding='utf-8') as f:
        content = f.read()
    
    # 替换from xxx为from xxx
    original_content = content
    content = re.sub(r'from backend\.', 'from ', content)
    
    # 如果有修改，写回文件
    if content != original_content:
        with open(filepath, 'w', encoding='utf-8') as f:
            f.write(content)
        print(f"✓ 修复: {filepath}")
        return True
    return False

def main():
    """主函数"""
    backend_dir = Path(__file__).parent
    fixed_count = 0
    
    # 遍历所有Python文件
    for py_file in backend_dir.rglob("*.py"):
        # 跳过虚拟环境和缓存目录
        if any(part in str(py_file) for part in ['venv', '__pycache__', '.git']):
            continue
        
        if fix_imports_in_file(py_file):
            fixed_count += 1
    
    print(f"\n总共修复了 {fixed_count} 个文件")

if __name__ == "__main__":
    main() 