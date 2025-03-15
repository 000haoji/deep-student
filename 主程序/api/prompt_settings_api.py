"""
学科Prompt设置相关API
"""
import os
import json
import logging
import re
from flask import Blueprint, request, jsonify, current_app
import config
import traceback

logger = logging.getLogger(__name__)

# 创建蓝图
prompt_settings_api = Blueprint('prompt_settings_api', __name__, url_prefix='/api')

# 默认提示词备份，以便重置
DEFAULT_PROMPTS = {}
for subject, prompt_data in config.SUBJECT_ANALYSIS_PROMPTS.items():
    DEFAULT_PROMPTS[subject] = {
        'deepseek': prompt_data.get('full_prompt', ''),
        'qwen': prompt_data.get('qwen_prompt', ''),
        'review': prompt_data.get('review_prompt', '')
    }

@prompt_settings_api.route('/get_prompt', methods=['GET'])
def get_prompt():
    """获取学科的分析提示词"""
    try:
        subject = request.args.get('subject')
        prompt_type = request.args.get('type', 'deepseek')  # 默认获取deepseek提示词
        
        if not subject:
            return jsonify({'error': '缺少必要参数'}), 400
            
        if subject not in config.SUBJECTS:
            return jsonify({'error': '学科不存在'}), 404
        
        prompt = ""
        if subject in config.SUBJECT_ANALYSIS_PROMPTS:
            if prompt_type == 'deepseek':
                prompt = config.SUBJECT_ANALYSIS_PROMPTS[subject].get('full_prompt', '')
            elif prompt_type == 'qwen':
                prompt = config.SUBJECT_ANALYSIS_PROMPTS[subject].get('qwen_prompt', '')
            elif prompt_type == 'review':
                prompt = config.SUBJECT_ANALYSIS_PROMPTS[subject].get('review_prompt', '')
        
        return jsonify({'prompt': prompt})
    except Exception as e:
        logger.error(f"获取提示词失败: {str(e)}")
        return jsonify({'error': f'获取失败: {str(e)}'}), 500

@prompt_settings_api.route('/save_prompt', methods=['POST'])
def save_prompt():
    """保存学科的分析提示词"""
    try:
        data = request.json
        subject = data.get('subject')
        prompt = data.get('prompt')
        prompt_type = data.get('type', 'deepseek')  # 默认为deepseek提示词
        
        if not subject or not prompt:
            return jsonify({'error': '缺少必要参数'}), 400
            
        if subject not in config.SUBJECTS:
            return jsonify({'error': '学科不存在'}), 404
            
        # 更新配置中的提示词
        if subject in config.SUBJECT_ANALYSIS_PROMPTS:
            if prompt_type == 'deepseek':
                config.SUBJECT_ANALYSIS_PROMPTS[subject]['full_prompt'] = prompt
            elif prompt_type == 'qwen':
                config.SUBJECT_ANALYSIS_PROMPTS[subject]['qwen_prompt'] = prompt
            elif prompt_type == 'review':
                config.SUBJECT_ANALYSIS_PROMPTS[subject]['review_prompt'] = prompt
        else:
            if prompt_type == 'deepseek':
                config.SUBJECT_ANALYSIS_PROMPTS[subject] = {'full_prompt': prompt}
            elif prompt_type == 'qwen':
                config.SUBJECT_ANALYSIS_PROMPTS[subject] = {'qwen_prompt': prompt}
            elif prompt_type == 'review':
                config.SUBJECT_ANALYSIS_PROMPTS[subject] = {'review_prompt': prompt}
            
        # 将更新后的提示词保存到文件
        save_prompts_to_file()
            
        return jsonify({'success': True, 'message': '提示词保存成功'})
    except Exception as e:
        logger.error(f"保存提示词失败: {str(e)}")
        return jsonify({'error': f'保存失败: {str(e)}'}), 500

@prompt_settings_api.route('/reset_prompt', methods=['POST'])
def reset_prompt():
    """重置学科的分析提示词为默认值"""
    try:
        data = request.json
        subject = data.get('subject')
        prompt_type = data.get('type', 'deepseek')  # 默认为deepseek提示词
        
        if not subject:
            return jsonify({'error': '缺少必要参数'}), 400
            
        if subject not in config.SUBJECTS:
            return jsonify({'error': '学科不存在'}), 404
            
        # 获取默认提示词
        default_prompt = DEFAULT_PROMPTS.get(subject, {}).get(prompt_type, '')
        
        # 更新配置中的提示词
        if subject in config.SUBJECT_ANALYSIS_PROMPTS:
            if prompt_type == 'deepseek':
                config.SUBJECT_ANALYSIS_PROMPTS[subject]['full_prompt'] = default_prompt
            elif prompt_type == 'qwen':
                config.SUBJECT_ANALYSIS_PROMPTS[subject]['qwen_prompt'] = default_prompt
            elif prompt_type == 'review':
                config.SUBJECT_ANALYSIS_PROMPTS[subject]['review_prompt'] = default_prompt
        else:
            if prompt_type == 'deepseek':
                config.SUBJECT_ANALYSIS_PROMPTS[subject] = {'full_prompt': default_prompt}
            elif prompt_type == 'qwen':
                config.SUBJECT_ANALYSIS_PROMPTS[subject] = {'qwen_prompt': default_prompt}
            elif prompt_type == 'review':
                config.SUBJECT_ANALYSIS_PROMPTS[subject] = {'review_prompt': default_prompt}
            
        # 将更新后的提示词保存到文件
        save_prompts_to_file()
            
        return jsonify({
            'success': True, 
            'message': '提示词已重置为默认值',
            'default_prompt': default_prompt
        })
    except Exception as e:
        logger.error(f"重置提示词失败: {str(e)}")
        return jsonify({'error': f'重置失败: {str(e)}'}), 500

@prompt_settings_api.route('/create_subject', methods=['POST'])
def create_subject():
    """创建新学科"""
    try:
        data = request.json
        subject_key = data.get('key')
        subject_name = data.get('name')
        subject_icon = data.get('icon', 'fa-book')
        
        # 获取提示词，支持两种格式：旧格式和新格式
        if 'prompts' in data and isinstance(data['prompts'], dict):
            # 新格式：提示词作为一个包含多种类型的对象
            prompts = data['prompts']
            subject_prompt_deepseek = prompts.get('deepseek', '')
            subject_prompt_qwen = prompts.get('qwen', '')
            subject_prompt_review = prompts.get('review', '')
        else:
            # 兼容旧格式
            subject_prompt_deepseek = data.get('prompt_deepseek', '')
            subject_prompt_qwen = data.get('prompt_qwen', '')
            subject_prompt_review = data.get('prompt_review', '')
        
        # 验证必要参数
        if not subject_key or not subject_name:
            return jsonify({'error': '学科标识符和名称不能为空'}), 400
            
        # 验证学科标识符格式
        if not re.match(r'^[a-zA-Z0-9_]+$', subject_key):
            return jsonify({'error': '学科标识符只能包含英文字母、数字和下划线'}), 400
            
        # 检查学科是否已存在
        if subject_key in config.SUBJECTS:
            return jsonify({'error': f'学科标识符 "{subject_key}" 已存在'}), 400
            
        # 确保包含review_prompt字段
        if 'review_prompt' not in data:
            subject_prompt_review = ''
        
        # 创建新学科
        config.SUBJECTS[subject_key] = {
            'name': subject_name,
            'icon': subject_icon,
            'enabled': True  # 设置为启用状态
        }
        
        # 如果提供了提示词，则添加到提示词配置中
        if subject_prompt_deepseek or subject_prompt_qwen or subject_prompt_review:
            config.SUBJECT_ANALYSIS_PROMPTS[subject_key] = {
                'full_prompt': subject_prompt_deepseek,
                'qwen_prompt': subject_prompt_qwen,
                'review_prompt': subject_prompt_review
            }
        
        # 保存配置
        save_subjects_to_file()
        save_prompts_to_file()
            
        return jsonify({
            'success': True, 
            'message': f'学科 "{subject_name}" 创建成功',
            'subject': {
                'key': subject_key,
                'name': subject_name,
                'icon': subject_icon
            }
        })
    except Exception as e:
        logger.error(f"创建新学科失败: {str(e)}")
        return jsonify({'error': f'创建失败: {str(e)}'}), 500

@prompt_settings_api.route('/delete_subject', methods=['POST'])
def delete_subject():
    """
    删除学科
    """
    try:
        data = request.get_json()
        if not data:
            return jsonify({'error': '无效的请求数据'}), 400
            
        # 获取学科标识符和确认级别
        subject_key = data.get('key')
        confirmation_level = data.get('confirmation_level', 1)
        
        if not subject_key:
            return jsonify({'error': '未提供学科标识符'}), 400
            
        # 检查学科是否存在
        if subject_key not in config.SUBJECTS:
            return jsonify({'error': f'学科 "{subject_key}" 不存在'}), 404
            
        # 如果确认级别小于3，则返回需要进一步确认的信息
        if confirmation_level < 3:
            return jsonify({
                'status': 'confirmation_required',
                'current_level': confirmation_level,
                'message': f'请确认是否要删除学科 "{config.SUBJECTS[subject_key]["name"]}"？这将删除该学科的所有设置和提示词配置。',
                'subject': {
                    'key': subject_key,
                    'name': config.SUBJECTS[subject_key]["name"]
                }
            }), 200
            
        # 确认级别达到3，执行删除操作
        if subject_key in config.SUBJECT_ANALYSIS_PROMPTS:
            del config.SUBJECT_ANALYSIS_PROMPTS[subject_key]
            
        del config.SUBJECTS[subject_key]
        
        # 保存配置到文件
        save_subjects_to_file()
        save_prompts_to_file()
        
        return jsonify({
            'status': 'success',
            'message': f'学科 "{subject_key}" 已成功删除'
        }), 200
        
    except Exception as e:
        current_app.logger.error(f"删除学科失败: {str(e)}")
        current_app.logger.error(traceback.format_exc())
        return jsonify({'error': str(e)}), 500

def save_prompts_to_file():
    """将提示词配置保存到文件，方便在系统重启后保持设置"""
    try:
        # 保存路径
        prompts_dir = os.path.join(os.path.dirname(os.path.dirname(os.path.abspath(__file__))), 'data')
        os.makedirs(prompts_dir, exist_ok=True)
        prompts_file = os.path.join(prompts_dir, 'subject_prompts.json')
        
        # 保存提示词配置
        with open(prompts_file, 'w', encoding='utf-8') as f:
            json.dump(config.SUBJECT_ANALYSIS_PROMPTS, f, ensure_ascii=False, indent=4)
            
        logger.info(f"提示词配置已保存到文件: {prompts_file}")
        return True
    except Exception as e:
        logger.error(f"保存提示词配置到文件失败: {str(e)}")
        return False

def save_subjects_to_file():
    """将学科配置保存到文件"""
    try:
        # 保存路径
        data_dir = os.path.join(os.path.dirname(os.path.dirname(os.path.abspath(__file__))), 'data')
        os.makedirs(data_dir, exist_ok=True)
        subjects_file = os.path.join(data_dir, 'subjects.json')
        
        # 保存学科配置
        with open(subjects_file, 'w', encoding='utf-8') as f:
            json.dump(config.SUBJECTS, f, ensure_ascii=False, indent=4)
            
        logger.info(f"学科配置已保存到文件: {subjects_file}")
        return True
    except Exception as e:
        logger.error(f"保存学科配置到文件失败: {str(e)}")
        return False
