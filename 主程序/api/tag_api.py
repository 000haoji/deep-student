"""
标签相关API
"""
import logging
import traceback
from flask import Blueprint, request, jsonify
from core import database

logger = logging.getLogger(__name__)

tag_api = Blueprint('tag_api', __name__)

@tag_api.route('/api/tags', methods=['GET'])
def get_tags():
    """获取所有标签"""
    try:
        tags = database.get_all_tags()
        return jsonify(tags)
    except Exception as e:
        logger.error(f"获取标签列表失败: {str(e)}\n{traceback.format_exc()}")
        return jsonify({"error": str(e)}), 500

@tag_api.route('/api/tags/suggest', methods=['GET'])
def suggest_tags():
    """推荐标签"""
    try:
        partial_tag = request.args.get('q', '')
        suggestions = database.suggest_tags(partial_tag)
        return jsonify(suggestions)
    except Exception as e:
        logger.error(f"标签推荐失败: {str(e)}\n{traceback.format_exc()}")
        return jsonify({"error": str(e)}), 500

@tag_api.route('/api/tags/merge', methods=['POST'])
def merge_tags():
    """合并标签"""
    try:
        data = request.json
        if not data or 'source_ids' not in data or 'target_id' not in data:
            return jsonify({"error": "请提供源标签ID和目标标签ID"}), 400
            
        source_ids = data['source_ids']
        target_id = data['target_id']
        
        if database.merge_tags(source_ids, target_id):
            return jsonify({"success": True, "message": "标签合并成功"})
        else:
            return jsonify({"success": False, "error": "标签合并失败"}), 500
    except Exception as e:
        logger.error(f"合并标签失败: {str(e)}\n{traceback.format_exc()}")
        return jsonify({"error": str(e)}), 500

@tag_api.route('/api/tags/create', methods=['POST'])
def create_tag():
    """创建新标签"""
    try:
        data = request.json
        if not data or 'name' not in data:
            return jsonify({"error": "请提供标签名称"}), 400
        
        tag_name = data['name']
        category = data.get('category', '通用知识点')
        
        conn = database.sqlite3.connect(database.DATABASE_PATH)
        cursor = conn.cursor()
        
        # 检查标签是否已存在
        cursor.execute("SELECT id FROM tags WHERE name = ?", (tag_name,))
        existing = cursor.fetchone()
        
        if existing:
            conn.close()
            return jsonify({"success": False, "error": "标签已存在", "tag_id": existing[0]}), 409
        
        # 创建新标签
        cursor.execute(
            '''
            INSERT INTO tags (name, category, created_at, usage_count)
            VALUES (?, ?, ?, 0)
            ''',
            (tag_name, category, database.datetime.datetime.now().isoformat())
        )
        
        tag_id = cursor.lastrowid
        conn.commit()
        conn.close()
        
        return jsonify({
            "success": True,
            "message": "标签创建成功",
            "tag_id": tag_id,
            "tag_name": tag_name,
            "category": category
        })
    except Exception as e:
        logger.error(f"创建标签失败: {str(e)}\n{traceback.format_exc()}")
        return jsonify({"error": str(e)}), 500

@tag_api.route('/api/tags/delete/<int:tag_id>', methods=['DELETE'])
def delete_tag(tag_id):
    """删除标签"""
    try:
        conn = database.sqlite3.connect(database.DATABASE_PATH)
        cursor = conn.cursor()
        
        # 检查标签是否存在
        cursor.execute("SELECT id FROM tags WHERE id = ?", (tag_id,))
        if not cursor.fetchone():
            conn.close()
            return jsonify({"error": "标签不存在"}), 404
        
        # 删除标签关联
        cursor.execute("DELETE FROM problem_tags WHERE tag_id = ?", (tag_id,))
        
        # 删除标签
        cursor.execute("DELETE FROM tags WHERE id = ?", (tag_id,))
        
        conn.commit()
        conn.close()
        
        return jsonify({"success": True, "message": "标签已删除"})
    except Exception as e:
        logger.error(f"删除标签失败: {str(e)}\n{traceback.format_exc()}")
        return jsonify({"error": str(e)}), 500