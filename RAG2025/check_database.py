#!/usr/bin/env python
# -*- coding: utf-8 -*-

"""
数据库检查工具 - 用于查询文档存储和向量存储的内部状态
"""

import os
import sys
import argparse
import json
import sqlitedict
from tabulate import tabulate
from datetime import datetime
import traceback

# 数据目录
DATA_DIR = os.path.join(os.path.dirname(os.path.abspath(__file__)), "data")
DOCUMENTS_PATH = os.path.join(DATA_DIR, "documents.sqlite")
VECTORS_PATH = os.path.join(DATA_DIR, "vectors.sqlite")
METADATA_PATH = os.path.join(DATA_DIR, "metadata.json")

def format_size(size_bytes):
    """格式化文件大小"""
    if size_bytes < 1024:
        return f"{size_bytes} B"
    elif size_bytes < 1024 * 1024:
        return f"{size_bytes/1024:.2f} KB"
    elif size_bytes < 1024 * 1024 * 1024:
        return f"{size_bytes/(1024*1024):.2f} MB"
    else:
        return f"{size_bytes/(1024*1024*1024):.2f} GB"

def check_database_files():
    """检查数据库文件状态"""
    print("\n===== 数据库文件状态 =====")
    
    files = [
        ("文档存储", DOCUMENTS_PATH),
        ("向量存储", VECTORS_PATH),
        ("元数据", METADATA_PATH)
    ]
    
    file_info = []
    for name, path in files:
        if os.path.exists(path):
            size = os.path.getsize(path)
            modified = datetime.fromtimestamp(os.path.getmtime(path)).strftime('%Y-%m-%d %H:%M:%S')
            file_info.append([name, "存在", format_size(size), modified])
        else:
            file_info.append([name, "不存在", "-", "-"])
    
    print(tabulate(file_info, headers=["文件类型", "状态", "大小", "最后修改时间"], tablefmt="grid"))

def count_documents():
    """统计文档数量"""
    print("\n===== 文档统计 =====")
    
    if not os.path.exists(DOCUMENTS_PATH):
        print("文档存储文件不存在！")
        return
    
    try:
        with sqlitedict.SqliteDict(DOCUMENTS_PATH) as doc_db:
            total_docs = len(doc_db)
            
            # 统计父文档和分段文档
            parent_docs = []
            fragment_docs = []
            
            for key in doc_db.keys():
                doc_data = doc_db[key]
                if "_chunk_" in key:
                    fragment_docs.append(key)
                else:
                    parent_docs.append(key)
            
            # 统计文档库
            libraries = {}
            for key in doc_db.keys():
                doc_data = doc_db[key]
                lib_id = doc_data.get("meta", {}).get("library_id", "未知")
                if lib_id not in libraries:
                    libraries[lib_id] = 0
                libraries[lib_id] += 1
            
            # 输出统计信息
            print(f"总文档数: {total_docs}")
            print(f"父文档数: {len(parent_docs)}")
            print(f"分段文档数: {len(fragment_docs)}")
            
            print("\n文档库分布:")
            lib_info = [[lib, count] for lib, count in libraries.items()]
            print(tabulate(lib_info, headers=["文档库ID", "文档数量"], tablefmt="grid"))
            
            return parent_docs, fragment_docs
    except Exception as e:
        print(f"统计文档时出错: {str(e)}")
        return [], []

def check_vectors():
    """检查向量存储"""
    print("\n===== 向量存储状态 =====")
    
    if not os.path.exists(VECTORS_PATH):
        print("向量存储文件不存在！")
        return
    
    try:
        with sqlitedict.SqliteDict(VECTORS_PATH) as vec_db:
            total_vectors = len(vec_db)
            
            # 获取一个样本向量的维度
            sample_dimension = None
            for key in vec_db.keys():
                vector = vec_db[key]
                if vector is not None:
                    sample_dimension = len(vector)
                    break
            
            print(f"总向量数: {total_vectors}")
            print(f"向量维度: {sample_dimension}")
            
            # 检查是否有空向量
            empty_vectors = 0
            for key in vec_db.keys():
                if vec_db[key] is None:
                    empty_vectors += 1
            
            print(f"空向量数: {empty_vectors}")
    except Exception as e:
        print(f"检查向量存储时出错: {str(e)}")

def list_documents(count=5, show_parents=True, show_fragments=False):
    """列出文档示例"""
    print(f"\n===== 文档示例 (最多显示{count}个) =====")
    
    if not os.path.exists(DOCUMENTS_PATH):
        print("文档存储文件不存在！")
        return
    
    try:
        with sqlitedict.SqliteDict(DOCUMENTS_PATH) as doc_db:
            docs_to_show = []
            
            # 收集要显示的文档
            for key in doc_db.keys():
                is_fragment = "_chunk_" in key
                if (is_fragment and show_fragments) or (not is_fragment and show_parents):
                    doc_data = doc_db[key]
                    meta = doc_data.get("meta", {})
                    
                    # 提取文档信息，确保内容是安全的字符串
                    content = doc_data.get("content", "")
                    if content:
                        # 移除可能导致显示问题的字符
                        content = content.replace("\r", " ").replace("\n", " ")
                        content = ''.join(c if c.isprintable() else ' ' for c in content)
                        content_preview = content[:50] + "..." if len(content) > 50 else content
                    else:
                        content_preview = "(无内容)"
                    
                    doc_info = {
                        "id": doc_data.get("id", key),
                        "type": "分段文档" if is_fragment else "父文档",
                        "title": meta.get("title", "未命名"),
                        "library_id": meta.get("library_id", "未知"),
                        "is_fragment": meta.get("is_fragment", is_fragment),
                        "content_preview": content_preview
                    }
                    docs_to_show.append(doc_info)
                    
                    if len(docs_to_show) >= count:
                        break
            
            # 显示文档信息
            if docs_to_show:
                doc_table = []
                for doc in docs_to_show:
                    doc_table.append([
                        doc["id"][:8] + "...",
                        doc["type"],
                        doc["title"],
                        doc["library_id"],
                        str(doc["is_fragment"]),
                        doc["content_preview"]
                    ])
                
                print(tabulate(doc_table, headers=["ID", "类型", "标题", "文档库", "is_fragment", "内容预览"], tablefmt="grid"))
            else:
                print("没有找到符合条件的文档")
    except Exception as e:
        print(f"列出文档时出错: {str(e)}")

def show_document_details(doc_id):
    """显示特定文档的详细信息"""
    print(f"\n===== 文档详情 [{doc_id}] =====")
    
    if not os.path.exists(DOCUMENTS_PATH):
        print("文档存储文件不存在！")
        return
    
    try:
        with sqlitedict.SqliteDict(DOCUMENTS_PATH) as doc_db:
            # 查找完全匹配的文档
            if doc_id in doc_db:
                doc_data = doc_db[doc_id]
                print(f"文档ID: {doc_id}")
                print(f"内容长度: {len(doc_data.get('content', ''))}")
                print("\n元数据:")
                meta = doc_data.get("meta", {})
                for key, value in meta.items():
                    print(f"  {key}: {value}")
                
                print("\n内容预览:")
                content = doc_data.get("content", "")
                if content:
                    # 移除可能导致显示问题的字符
                    preview = content[:500] + "..." if len(content) > 500 else content
                    preview = preview.replace("\r", "\n")
                    # 确保内容可打印
                    preview = ''.join(c if c.isprintable() else ' ' for c in preview)
                    print(preview)
                else:
                    print("(无内容)")
                
                # 检查是否有向量
                with sqlitedict.SqliteDict(VECTORS_PATH) as vec_db:
                    if doc_id in vec_db:
                        vector = vec_db[doc_id]
                        print(f"\n向量: {'存在' if vector is not None else '为空'}")
                        if vector is not None:
                            print(f"向量维度: {len(vector)}")
                    else:
                        print("\n向量: 不存在")
                
                return
            
            # 查找前缀匹配的文档
            matching_docs = [k for k in doc_db.keys() if k.startswith(doc_id)]
            if matching_docs:
                print(f"找到 {len(matching_docs)} 个ID前缀匹配的文档:")
                for i, match_id in enumerate(matching_docs[:5]):
                    doc_data = doc_db[match_id]
                    meta = doc_data.get("meta", {})
                    print(f"{i+1}. {match_id} - {meta.get('title', '未命名')}")
                
                if len(matching_docs) > 5:
                    print(f"... 以及 {len(matching_docs) - 5} 个更多匹配项")
                
                print("\n请使用完整的文档ID查看详情")
                return
            
            print(f"未找到ID为 {doc_id} 的文档")
    except Exception as e:
        print(f"显示文档详情时出错: {str(e)}")

def simple_list_documents(count=10):
    """以简单格式列出文档"""
    print(f"\n===== 简易文档列表 (最多显示{count}个) =====")
    
    if not os.path.exists(DOCUMENTS_PATH):
        print("文档存储文件不存在！")
        return
    
    try:
        with sqlitedict.SqliteDict(DOCUMENTS_PATH) as doc_db:
            print(f"数据库中共有 {len(doc_db)} 个文档记录")
            
            # 分别统计父文档和分段文档
            parent_docs = []
            fragment_docs = []
            
            for key in doc_db.keys():
                doc_data = doc_db[key]
                meta = doc_data.get("meta", {})
                is_fragment = meta.get("is_fragment", "_chunk_" in key)
                
                doc_info = {
                    "id": key,
                    "title": meta.get("title", "未命名"),
                    "is_fragment": is_fragment
                }
                
                if is_fragment:
                    fragment_docs.append(doc_info)
                else:
                    parent_docs.append(doc_info)
            
            # 显示父文档
            print(f"\n父文档数量: {len(parent_docs)}")
            for i, doc in enumerate(parent_docs[:count]):
                print(f"{i+1}. ID: {doc['id'][:15]}... | 标题: {doc['title']}")
            
            if len(parent_docs) > count:
                print(f"... 以及 {len(parent_docs) - count} 个更多父文档")
            
            # 显示分段文档
            print(f"\n分段文档数量: {len(fragment_docs)}")
            for i, doc in enumerate(fragment_docs[:count]):
                print(f"{i+1}. ID: {doc['id'][:15]}... | 标题: {doc['title']}")
            
            if len(fragment_docs) > count:
                print(f"... 以及 {len(fragment_docs) - count} 个更多分段文档")
    
    except Exception as e:
        print(f"列出文档时出错: {str(e)}")
        traceback.print_exc()

def main():
    parser = argparse.ArgumentParser(description="检查文档和向量数据库")
    parser.add_argument("--status", action="store_true", help="显示数据库文件状态")
    parser.add_argument("--stats", action="store_true", help="显示文档统计信息")
    parser.add_argument("--vectors", action="store_true", help="检查向量存储")
    parser.add_argument("--list", action="store_true", help="列出文档示例")
    parser.add_argument("--simple-list", action="store_true", help="以简单格式列出文档")
    parser.add_argument("--count", type=int, default=5, help="要显示的文档数量")
    parser.add_argument("--parents", action="store_true", help="只显示父文档")
    parser.add_argument("--fragments", action="store_true", help="只显示分段文档")
    parser.add_argument("--doc-id", type=str, help="要显示详情的文档ID")
    parser.add_argument("--all", action="store_true", help="显示所有信息")
    
    args = parser.parse_args()
    
    # 如果没有指定任何参数，显示帮助信息
    if not any(vars(args).values()):
        parser.print_help()
        return
    
    # 处理--all参数
    if args.all:
        args.status = True
        args.stats = True
        args.vectors = True
        args.simple_list = True
    
    # 执行请求的操作
    if args.status:
        check_database_files()
    
    if args.stats:
        count_documents()
    
    if args.vectors:
        check_vectors()
    
    if args.list:
        # 如果没有指定父文档或分段文档，默认显示父文档
        show_parents = not args.fragments if args.parents or not args.fragments else False
        show_fragments = args.fragments
        list_documents(args.count, show_parents, show_fragments)
    
    if args.simple_list:
        simple_list_documents(args.count)
    
    if args.doc_id:
        show_document_details(args.doc_id)

if __name__ == "__main__":
    main()
