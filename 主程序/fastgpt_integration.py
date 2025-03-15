import requests
import json
import logging
from flask import Blueprint, request, jsonify, Response, stream_with_context

# 配置日志
logging.basicConfig(level=logging.INFO)
logger = logging.getLogger(__name__)

fastgpt_bp = Blueprint('fastgpt', __name__)

class FastGPTIntegration:
    def __init__(self, api_key=None, base_url="https://api.fastgpt.in/api"):
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
            url = f"{self.base_url}/kb/chat/completions"
            
            payload = {
                "model": "gpt-3.5-turbo",  # 可配置
                "messages": [{"role": "user", "content": query}],
                "knowledge_base_id": kb_id,
                "stream": stream
            }
            
            logger.info(f"查询FastGPT知识库: {kb_id}")
            
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
                    logger.error(f"FastGPT API错误: {response.status_code} - {response.text}")
                    return None
            else:
                response = requests.post(url, headers=self.headers, json=payload)
                
                if response.status_code == 200:
                    return response.json()
                else:
                    logger.error(f"FastGPT API错误: {response.status_code} - {response.text}")
                    return None
        except Exception as e:
            logger.error(f"查询FastGPT知识库时发生错误: {str(e)}")
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

# 初始化FastGPT集成实例
fastgpt_client = FastGPTIntegration()

@fastgpt_bp.route('/api/fastgpt/settings', methods=['GET', 'POST'])
def fastgpt_settings():
    """获取或更新FastGPT设置"""
    if request.method == 'POST':
        data = request.json
        api_key = data.get('api_key')
        base_url = data.get('base_url', "https://api.fastgpt.in/api")
        
        # 更新设置
        fastgpt_client.set_api_key(api_key)
        fastgpt_client.base_url = base_url
        
        # 可以保存设置到配置文件
        settings = {
            'api_key': api_key,
            'base_url': base_url
        }
        # TODO: 保存设置到配置文件
        
        return jsonify({"status": "success", "message": "FastGPT设置已更新"})
    else:
        # TODO: 从配置文件加载设置
        settings = {
            'api_key': fastgpt_client.api_key,
            'base_url': fastgpt_client.base_url
        }
        return jsonify(settings)

@fastgpt_bp.route('/api/fastgpt/knowledge_bases', methods=['GET'])
def list_knowledge_bases():
    """获取知识库列表"""
    result = fastgpt_client.list_knowledge_bases()
    if result:
        return jsonify(result)
    else:
        return jsonify({"status": "error", "message": "获取知识库列表失败"}), 500

@fastgpt_bp.route('/api/fastgpt/query', methods=['POST'])
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
            result_stream = fastgpt_client.query_knowledge_base(kb_id, query, stream=True)
            if result_stream:
                for line in result_stream:
                    if line:
                        yield f"data: {line.decode('utf-8')}\n\n"
            else:
                yield f"data: {json.dumps({'status': 'error', 'message': '查询失败'})}\n\n"
        
        return Response(stream_with_context(generate()), content_type='text/event-stream')
    else:
        result = fastgpt_client.query_knowledge_base(kb_id, query, stream=False)
        if result:
            return jsonify(result)
        else:
            return jsonify({"status": "error", "message": "查询失败"}), 500 