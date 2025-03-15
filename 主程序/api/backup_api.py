"""
数据备份和恢复相关API
"""
import os
import logging
import traceback
import datetime
from flask import Blueprint, request, jsonify, send_file
from core import database
import json
import time
import shutil
import sqlite3
import config
from core.database import get_db
from werkzeug.utils import secure_filename

logger = logging.getLogger(__name__)

backup_api = Blueprint('backup_api', __name__)

# 确保备份目录存在
os.makedirs(config.BACKUP_FOLDER, exist_ok=True)

@backup_api.route('/api/backup/history', methods=['GET'])
def get_backup_history():
    """获取备份历史"""
    try:
        backups = []
        
        # 确保备份目录存在
        if not os.path.exists(config.BACKUP_FOLDER):
            os.makedirs(config.BACKUP_FOLDER)
            
        # 获取所有备份文件
        for filename in os.listdir(config.BACKUP_FOLDER):
            if filename.endswith('.db'):
                file_path = os.path.join(config.BACKUP_FOLDER, filename)
                file_stats = os.stat(file_path)
                
                # 从文件名中提取信息
                name_parts = filename.split('_')
                date_str = '_'.join(name_parts[-2:]).replace('.db', '')
                
                try:
                    # 尝试解析日期
                    date_obj = datetime.datetime.strptime(date_str, '%Y%m%d_%H%M%S')
                    date_formatted = date_obj.strftime('%Y-%m-%d %H:%M:%S')
                except:
                    date_formatted = "未知日期"
                
                # 提取备份名称
                backup_name = '_'.join(name_parts[:-2]) if len(name_parts) > 2 else filename.replace('.db', '')
                
                backups.append({
                    'id': filename,
                    'name': backup_name,
                    'date': date_formatted,
                    'size': f"{file_stats.st_size / (1024 * 1024):.2f} MB",
                    'path': file_path
                })
        
        # 按日期排序，最新的在前面
        backups.sort(key=lambda x: x['date'], reverse=True)
        
        return jsonify({
            'success': True,
            'backups': backups
        })
    except Exception as e:
        logger.error(f"获取备份历史失败: {str(e)}")
        return jsonify({
            'success': False,
            'error': f"获取备份历史失败: {str(e)}"
        }), 500

@backup_api.route('/api/backup/create', methods=['POST'])
def create_backup():
    """创建数据库备份"""
    try:
        data = request.get_json()
        backup_name = data.get('name', f'backup_{datetime.datetime.now().strftime("%Y%m%d_%H%M%S")}')
        
        # 确保备份目录存在
        if not os.path.exists(config.BACKUP_FOLDER):
            os.makedirs(config.BACKUP_FOLDER)
        
        # 创建备份文件名
        backup_filename = f"{secure_filename(backup_name)}_{datetime.datetime.now().strftime('%Y%m%d_%H%M%S')}.db"
        backup_path = os.path.join(config.BACKUP_FOLDER, backup_filename)
        
        # 复制数据库文件
        shutil.copy2(config.DATABASE_PATH, backup_path)
        
        return jsonify({
            'success': True,
            'message': '备份创建成功',
            'backup_id': os.path.basename(backup_path),
            'backup_path': backup_path
        })
    except Exception as e:
        logger.error(f"创建备份失败: {str(e)}")
        return jsonify({
            'success': False,
            'error': f"创建备份失败: {str(e)}"
        }), 500

@backup_api.route('/api/backup/restore', methods=['POST'])
def restore_backup():
    """从上传的文件恢复备份"""
    try:
        if 'file' not in request.files:
            return jsonify({
                'success': False,
                'error': '没有上传文件'
            }), 400
            
        file = request.files['file']
        
        if file.filename == '':
            return jsonify({
                'success': False,
                'error': '没有选择文件'
            }), 400
            
        if not file.filename.endswith('.db'):
            return jsonify({
                'success': False,
                'error': '不支持的文件格式，请上传.db文件'
            }), 400
            
        # 创建当前数据库的安全备份
        safety_backup = os.path.join(
            config.BACKUP_FOLDER, 
            f"safety_backup_{datetime.datetime.now().strftime('%Y%m%d_%H%M%S')}.db"
        )
        shutil.copy2(config.DATABASE_PATH, safety_backup)
        
        # 保存上传的文件
        temp_path = os.path.join(config.BACKUP_FOLDER, 'temp_restore.db')
        file.save(temp_path)
        
        # 验证文件是否为有效的SQLite数据库
        try:
            conn = sqlite3.connect(temp_path)
            conn.close()
        except:
            os.remove(temp_path)
            return jsonify({
                'success': False,
                'error': '上传的文件不是有效的SQLite数据库'
            }), 400
            
        # 恢复备份
        shutil.copy2(temp_path, config.DATABASE_PATH)
        os.remove(temp_path)
        
        return jsonify({
            'success': True,
            'message': '备份恢复成功',
            'safety_backup': os.path.basename(safety_backup)
        })
    except Exception as e:
        logger.error(f"恢复备份失败: {str(e)}")
        return jsonify({
            'success': False,
            'error': f"恢复备份失败: {str(e)}"
        }), 500

@backup_api.route('/api/backup/restore/<backup_id>', methods=['POST'])
def restore_backup_by_id(backup_id):
    """从已有备份恢复"""
    try:
        backup_path = os.path.join(config.BACKUP_FOLDER, backup_id)
        
        if not os.path.exists(backup_path):
            return jsonify({
                'success': False,
                'error': '备份文件不存在'
            }), 404
        
        # 创建当前数据库的安全备份
        safety_backup = os.path.join(
            config.BACKUP_FOLDER, 
            f"safety_backup_{datetime.datetime.now().strftime('%Y%m%d_%H%M%S')}.db"
        )
        shutil.copy2(config.DATABASE_PATH, safety_backup)
        
        # 恢复备份
        shutil.copy2(backup_path, config.DATABASE_PATH)
        
        return jsonify({
            'success': True,
            'message': '备份恢复成功',
            'safety_backup': os.path.basename(safety_backup)
        })
    except Exception as e:
        logger.error(f"恢复备份失败: {str(e)}")
        return jsonify({
            'success': False,
            'error': f"恢复备份失败: {str(e)}"
        }), 500

@backup_api.route('/api/backup/download/<backup_id>', methods=['GET'])
def download_backup(backup_id):
    """下载备份文件"""
    try:
        backup_path = os.path.join(config.BACKUP_FOLDER, backup_id)
        
        if not os.path.exists(backup_path):
            return jsonify({
                'success': False,
                'error': '备份文件不存在'
            }), 404
            
        return send_file(
            backup_path,
            as_attachment=True,
            download_name=backup_id
        )
    except Exception as e:
        logger.error(f"下载备份失败: {str(e)}")
        return jsonify({
            'success': False,
            'error': f"下载备份失败: {str(e)}"
        }), 500

@backup_api.route('/api/backup/delete/<backup_id>', methods=['DELETE'])
def delete_backup(backup_id):
    """删除备份文件"""
    try:
        backup_path = os.path.join(config.BACKUP_FOLDER, backup_id)
        
        if not os.path.exists(backup_path):
            return jsonify({
                'success': False,
                'error': '备份文件不存在'
            }), 404
            
        os.remove(backup_path)
        
        return jsonify({
            'success': True,
            'message': '备份删除成功'
        })
    except Exception as e:
        logger.error(f"删除备份失败: {str(e)}")
        return jsonify({
            'success': False,
            'error': f"删除备份失败: {str(e)}"
        }), 500

@backup_api.route('/api/backup/settings', methods=['GET'])
def get_backup_settings():
    """获取自动备份设置"""
    try:
        conn = get_db()
        cursor = conn.cursor()
        
        # 查询设置
        cursor.execute("""
            SELECT value FROM settings 
            WHERE category = 'backup' AND key = 'auto_settings'
        """)
        
        result = cursor.fetchone()
        
        if result:
            settings = json.loads(result[0])
        else:
            settings = {
                'enabled': False,
                'frequency': 'weekly',
                'retention': 5
            }
            
        return jsonify(settings)
    except Exception as e:
        logger.error(f"获取备份设置失败: {str(e)}")
        return jsonify({
            'success': False,
            'error': f"获取备份设置失败: {str(e)}"
        }), 500

@backup_api.route('/api/backup/settings', methods=['POST'])
def save_backup_settings():
    """保存自动备份设置"""
    try:
        data = request.get_json()
        
        settings = {
            'enabled': data.get('enabled', False),
            'frequency': data.get('frequency', 'weekly'),
            'retention': int(data.get('retention', 5))
        }
        
        conn = get_db()
        cursor = conn.cursor()
        
        # 保存设置
        cursor.execute("""
            INSERT OR REPLACE INTO settings (category, key, value, updated_at)
            VALUES (?, ?, ?, ?)
        """, ('backup', 'auto_settings', json.dumps(settings), datetime.datetime.now().strftime('%Y-%m-%d %H:%M:%S')))
        
        conn.commit()
        
        return jsonify({
            'success': True,
            'message': '备份设置保存成功'
        })
    except Exception as e:
        logger.error(f"保存备份设置失败: {str(e)}")
        return jsonify({
            'success': False,
            'error': f"保存备份设置失败: {str(e)}"
        }), 500

# 清理旧备份
def cleanup_old_backups(retention=5):
    """清理旧备份，保留指定数量的最新备份"""
    try:
        backups = []
        
        # 获取所有备份文件
        for filename in os.listdir(config.BACKUP_FOLDER):
            if filename.endswith('.sqlite'):
                file_path = os.path.join(config.BACKUP_FOLDER, filename)
                file_stats = os.stat(file_path)
                
                backups.append({
                    'filename': filename,
                    'path': file_path,
                    'mtime': file_stats.st_mtime
                })
        
        # 按修改时间排序
        backups.sort(key=lambda x: x['mtime'], reverse=True)
        
        # 删除超出保留数量的旧备份
        if len(backups) > retention:
            for backup in backups[retention:]:
                os.remove(backup['path'])
                logger.info(f"清理旧备份: {backup['filename']}")
        
        return True
    except Exception as e:
        logger.error(f"清理旧备份失败: {str(e)}")
        return False
