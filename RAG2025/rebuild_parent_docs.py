#!/usr/bin/env python
# -*- coding: utf-8 -*-

"""
重建父文档脚本 - 从现有的分段文档中重建父文档
"""

import os
import sys
import sqlitedict
from haystack import Document
import traceback
from collections import defaultdict

# 导入项目配置
sys.path.append(os.path.dirname(os.path.abspath(__file__)))
from core.config import DATA_DIR, DOCUMENTS_PATH, VECTORS_PATH
from core.storage import save_documents

def rebuild_parent_documents():
    """从现有的分段文档中重建父文档"""
    print("\n===== 开始重建父文档 =====")
    
    if not os.path.exists(DOCUMENTS_PATH):
        print(f"文档存储文件不存在: {DOCUMENTS_PATH}")
        return False
    
    try:
        # 加载所有文档
        with sqlitedict.SqliteDict(DOCUMENTS_PATH) as doc_db:
            print(f"数据库中共有 {len(doc_db)} 个文档记录")
            
            # 按照文档ID前缀分组
            doc_groups = defaultdict(list)
            parent_docs = {}
            
            for key, doc_data in doc_db.items():
                # 提取基础ID（去掉_chunk_部分）
                base_id = key.split("_chunk_")[0] if "_chunk_" in key else key
                is_fragment = "_chunk_" in key
                
                # 将文档添加到对应的分组
                doc_groups[base_id].append((key, doc_data, is_fragment))
                
                # 如果已经是父文档，直接记录
                if not is_fragment:
                    parent_docs[base_id] = (key, doc_data)
            
            # 统计信息
            print(f"找到 {len(doc_groups)} 个不同的文档组")
            print(f"其中 {len(parent_docs)} 个已有父文档")
            
            # 需要重建的文档组
            rebuild_groups = {base_id: docs for base_id, docs in doc_groups.items() if base_id not in parent_docs}
            print(f"需要重建 {len(rebuild_groups)} 个父文档")
            
            # 重建父文档
            rebuilt_docs = []
            for base_id, docs in rebuild_groups.items():
                # 按照片段索引排序
                sorted_docs = sorted(docs, key=lambda x: int(x[0].split("_chunk_")[1]) if "_chunk_" in x[0] else 0)
                
                if not sorted_docs:
                    continue
                
                # 使用第一个分段的数据作为父文档基础
                first_key, first_data, _ = sorted_docs[0]
                
                # 创建父文档
                parent_content = "".join([d[1]["content"] for d in sorted_docs])
                parent_meta = first_data.get("meta", {}).copy()
                
                # 提取文件名和标题
                filename = parent_meta.get("filename", "")
                title = filename if filename else "未命名文档"
                
                # 设置父文档元数据
                parent_meta["is_fragment"] = False
                parent_meta["has_fragments"] = True
                parent_meta["rebuilt"] = True
                parent_meta["original_fragments"] = [d[0] for d in sorted_docs]
                parent_meta["title"] = title
                parent_meta["filename"] = filename
                
                # 创建父文档对象
                parent_doc = Document(
                    id=base_id,
                    content=parent_content,
                    meta=parent_meta
                )
                
                rebuilt_docs.append(parent_doc)
                print(f"重建父文档: {base_id} (来自 {len(sorted_docs)} 个分段) - 标题: {title}")
            
            # 保存重建的父文档
            if rebuilt_docs:
                save_documents(rebuilt_docs)
                print(f"成功保存 {len(rebuilt_docs)} 个重建的父文档")
                return True
            else:
                print("没有需要重建的父文档")
                return False
    
    except Exception as e:
        print(f"重建父文档时出错: {str(e)}")
        traceback.print_exc()
        return False

if __name__ == "__main__":
    rebuild_parent_documents()
