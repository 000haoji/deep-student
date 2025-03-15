import requests
import json
import logging
from flask import Blueprint, request, jsonify, Response, stream_with_context

# 配置日志
logging.basicConfig(level=logging.INFO)
logger = logging.getLogger(__name__)

cherrystudio_bp = Blueprint('cherrystudio', __name__)

class CherryStudioIntegration:
    def __init__(self, api_key=None, base_url="https://api.cherrystudio.cn/api"):
        self.api_key = api_key
        self.base_url = base_url
        self.headers = {
            "Content-Type": "application/json"
        }
        if api_key:
            self.headers["Authorization"] = f"Bearer {api_key}"
    
    def set_api_key(self, api_key):
        """设置API密钥"""
        self.api_key = api_key
        self.headers["Authorization"] = f"Bearer {api_key}"
    
    def query_knowledge_base(self, kb_id, query, stream=True):
        """查询知识库"""
        try:
            url = f"{self.base_url}/kb/query"
            
            payload = {
                "knowledge_base_id": kb_id,
                "query": query,
                "stream": stream
            }
            
            logger.info(f"查询CherryStudio知识库: {kb_id}")
            
            if stream:
                response = requests.post(
                    url, 
                    headers=self.headers, 
                    json=payload,
                    stream=True
                )
                
                if response.status_code == 200:
                    return response.iter_lines()
                else:
                    logger.error(f"CherryStudio API错误: {response.status_code} - {response.text}")
                    return None
            else:
                response = requests.post(url, headers=self.headers, json=payload)
                
                if response.status_code == 200:
                    return response.json()
                else:
                    logger.error(f"CherryStudio API错误: {response.status_code} - {response.text}")
                    return None
        except Exception as e:
            logger.error(f"查询CherryStudio知识库时发生错误: {str(e)}")
            return None
    
    def list_knowledge_bases(self):
        """获取知识库列表"""
        try:
            url = f"{self.base_url}/kb/list"
            response = requests.get(url, headers=self.headers)
            
            if response.status_code == 200:
                return response.json()
            else:
                logger.error(f"获取知识库列表错误: {response.status_code} - {response.text}")
                return None
        except Exception as e:
            logger.error(f"获取知识库列表时发生错误: {str(e)}")
            return None
    
    def upload_document(self, kb_id, file_path, file_name=None):
        """上传文档到知识库"""
        try:
            url = f"{self.base_url}/kb/document/upload"
            
            if not file_name:
                import os
                file_name = os.path.basename(file_path)
            
            with open(file_path, 'rb') as f:
                files = {
                    'file': (file_name, f, 'application/octet-stream')
                }
                
                data = {
                    'knowledge_base_id': kb_id
                }
                
                # 上传文件时不使用默认headers
                upload_headers = {}
                if self.api_key:
                    upload_headers["Authorization"] = f"Bearer {self.api_key}"
                
                response = requests.post(url, headers=upload_headers, data=data, files=files)
                
                if response.status_code == 200:
                    return response.json()
                else:
                    logger.error(f"上传文档错误: {response.status_code} - {response.text}")
                    return None
        except Exception as e:
            logger.error(f"上传文档时发生错误: {str(e)}")
            return None

# 初始化CherryStudio集成实例
cherrystudio_client = CherryStudioIntegration()

@cherrystudio_bp.route('/api/cherrystudio/settings', methods=['GET', 'POST'])
def cherrystudio_settings():
    """获取或更新CherryStudio设置"""
    if request.method == 'POST':
        data = request.json
        api_key = data.get('api_key')
        base_url = data.get('base_url', "https://api.cherrystudio.cn/api")
        
        # 更新设置
        cherrystudio_client.set_api_key(api_key)
        cherrystudio_client.base_url = base_url
        
        # 可以保存设置到配置文件
        settings = {
            'api_key': api_key,
            'base_url': base_url
        }
        # TODO: 保存设置到配置文件
        
        return jsonify({"status": "success", "message": "CherryStudio设置已更新"})
    else:
        # TODO: 从配置文件加载设置
        settings = {
            'api_key': cherrystudio_client.api_key,
            'base_url': cherrystudio_client.base_url
        }
        return jsonify(settings)

@cherrystudio_bp.route('/api/cherrystudio/knowledge_bases', methods=['GET'])
def list_knowledge_bases():
    """获取知识库列表"""
    result = cherrystudio_client.list_knowledge_bases()
    if result:
        return jsonify(result)
    else:
        return jsonify({"status": "error", "message": "获取知识库列表失败"}), 500

@cherrystudio_bp.route('/api/cherrystudio/query', methods=['POST'])
def query_knowledge_base():
    """查询知识库"""
    data = request.json
    kb_id = data.get('kb_id')
    query = data.get('query')
    stream = data.get('stream', True)
    
    if not kb_id or not query:
        return jsonify({"status": "error", "message": "缺少必要参数"}), 400
    
    if stream:
        def generate():
            result_stream = cherrystudio_client.query_knowledge_base(kb_id, query, stream=True)
            if result_stream:
                for line in result_stream:
                    if line:
                        yield f"data: {line.decode('utf-8')}\n\n"
            else:
                yield f"data: {json.dumps({'status': 'error', 'message': '查询失败'})}\n\n"
        
        return Response(stream_with_context(generate()), content_type='text/event-stream')
    else:
        result = cherrystudio_client.query_knowledge_base(kb_id, query, stream=False)
        if result:
            return jsonify(result)
        else:
            return jsonify({"status": "error", "message": "查询失败"}), 500

@cherrystudio_bp.route('/api/cherrystudio/upload', methods=['POST'])
def upload_document():
    """上传文档到知识库"""
    if 'file' not in request.files:
        return jsonify({"status": "error", "message": "没有文件上传"}), 400
    
    file = request.files['file']
    kb_id = request.form.get('kb_id')
    
    if not kb_id:
        return jsonify({"status": "error", "message": "缺少知识库ID"}), 400
    
    if file.filename == '':
        return jsonify({"status": "error", "message": "未选择文件"}), 400
    
    # 保存上传的文件到临时位置
    import os
    import tempfile
    
    temp_dir = tempfile.gettempdir()
    temp_file_path = os.path.join(temp_dir, file.filename)
    file.save(temp_file_path)
    
    try:
        # 上传文件到CherryStudio
        result = cherrystudio_client.upload_document(kb_id, temp_file_path, file.filename)
        
        # 删除临时文件
        os.remove(temp_file_path)
        
        if result:
            return jsonify(result)
        else:
            return jsonify({"status": "error", "message": "上传文档失败"}), 500
    except Exception as e:
        # 确保临时文件被删除
        if os.path.exists(temp_file_path):
            os.remove(temp_file_path)
        
        logger.error(f"上传文档时发生错误: {str(e)}")
        return jsonify({"status": "error", "message": f"上传文档时发生错误: {str(e)}"}), 500 