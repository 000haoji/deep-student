import os
import sys
import json
import sqlitedict
from haystack import Document
from core.storage import load_documents_and_embeddings
from core.config import logger, DATA_DIR, DOCUMENTS_PATH

DB_PATH = os.path.join(DATA_DIR, "documents.sqlite")
DOCUMENT_STORE_PATH = DOCUMENTS_PATH

def check_database_status():
    """检查数据库状态，显示所有文档及其元数据"""
    print("\n===== 数据库状态检查 =====")
    
    # 检查数据库文件是否存在
    if not os.path.exists(DB_PATH):
        print(f"数据库文件不存在: {DB_PATH}")
        return
    
    print(f"数据库文件路径: {DB_PATH}")
    print(f"文档存储路径: {DOCUMENT_STORE_PATH}")
    
    # 从持久化存储加载文档
    documents = load_documents_and_embeddings(skip_embeddings=True)
    
    if not documents:
        print("数据库中没有文档")
        return
    
    # 统计文档类型
    parent_docs = []
    fragment_docs = []
    
    for doc in documents:
        if doc.meta.get("is_fragment", False):
            fragment_docs.append(doc)
        else:
            parent_docs.append(doc)
    
    print(f"\n总文档数: {len(documents)}")
    print(f"父文档数: {len(parent_docs)}")
    print(f"分段文档数: {len(fragment_docs)}")
    
    # 检查是否有重复的父文档
    filename_to_docs = {}
    for doc in parent_docs:
        filename = doc.meta.get('filename', '')
        if filename:
            if filename not in filename_to_docs:
                filename_to_docs[filename] = []
            filename_to_docs[filename].append(doc)
    
    duplicate_files = {filename: docs for filename, docs in filename_to_docs.items() if len(docs) > 1}
    
    # 显示所有父文档详情
    print("\n===== 父文档详情 =====")
    for i, doc in enumerate(parent_docs):
        print(f"\n文档 {i+1}:")
        print(f"  ID: {doc.id}")
        print(f"  标题: {doc.meta.get('title', '无标题')}")
        print(f"  文件名: {doc.meta.get('filename', '无文件名')}")
        print(f"  是否为分段: {doc.meta.get('is_fragment', False)}")
        print(f"  是否有分段: {doc.meta.get('has_fragments', False)}")
        print(f"  上传时间: {doc.meta.get('uploaded_at', '未知')}")
        
        # 统计该文档的分段数量
        fragment_count = sum(1 for d in fragment_docs if d.meta.get("parent_id") == doc.id)
        print(f"  分段数量: {fragment_count}")
        
        # 显示文档内容前100个字符
        content = doc.content if hasattr(doc, 'content') and doc.content else ""
        print(f"  内容预览: {content[:100]}..." if content else "  内容预览: 无内容")
    
    # 显示重复文档信息
    if duplicate_files:
        print("\n===== 重复文档检测 =====")
        print(f"发现 {len(duplicate_files)} 个重复的文件名:")
        for filename, docs in duplicate_files.items():
            print(f"\n  文件名: {filename}, 重复数量: {len(docs)}")
            for doc in docs:
                print(f"    - ID: {doc.id}")
                print(f"      标题: {doc.meta.get('title', '无标题')}")
                print(f"      上传时间: {doc.meta.get('uploaded_at', '未知')}")
                print(f"      内容长度: {len(doc.content) if hasattr(doc, 'content') and doc.content else 0}")
                
                # 显示文档内容前50个字符，用于比较是否真的重复
                content = doc.content if hasattr(doc, 'content') and doc.content else ""
                print(f"      内容预览: {content[:50]}..." if content else "      内容预览: 无内容")
    
    # 直接检查SQLite数据库中的文档
    print("\n===== SQLite数据库检查 =====")
    try:
        with sqlitedict.SqliteDict(DOCUMENTS_PATH, autocommit=False) as doc_db:
            # 获取所有键
            doc_keys = list(doc_db.keys())
            print(f"SQLite数据库中的文档数: {len(doc_keys)}")
            
            # 显示所有文档ID
            print("\n文档ID列表:")
            for key in doc_keys:
                doc_data = doc_db[key]
                if isinstance(doc_data, dict):
                    is_fragment = doc_data.get("meta", {}).get("is_fragment", False)
                    filename = doc_data.get("meta", {}).get("filename", "未知")
                    print(f"  - ID: {key}, 是否为分段: {is_fragment}, 文件名: {filename}")
                else:
                    print(f"  - ID: {key}, 数据类型: {type(doc_data)}")
    except Exception as e:
        print(f"访问SQLite数据库时出错: {str(e)}")

if __name__ == "__main__":
    check_database_status()
