from flask import Blueprint, render_template, request, Response, stream_with_context
import time
import json

# 创建Blueprint
markdown_test_api = Blueprint('markdown_test_api', __name__)

@markdown_test_api.route('/markdown-latex-test')
def markdown_latex_test_page():
    """渲染Markdown和LaTeX测试页面"""
    return render_template('markdown_latex_test.html')

@markdown_test_api.route('/api/test/markdown-latex-stream', methods=['POST'])
def stream_markdown_latex_test():
    """模拟流式输出API，用于测试Markdown和LaTeX渲染"""
    try:
        # 获取请求数据
        data = request.json
        content = data.get('content', '')
        speed = int(data.get('speed', 50)) / 1000  # 转换为秒
        
        # 创建流式响应
        def generate():
            chars = list(content)
            
            # 发送起始消息
            yield 'data: {"type": "start"}\n\n'
            
            # 逐字符发送数据，使用与DeepSeek格式相同的结构
            for char in chars:
                # 构建与stream_analysis.js中处理的格式一致的响应
                response_data = {
                    "id": "test_stream",
                    "model": "test_model",
                    "choices": [
                        {
                            "index": 0,
                            "delta": {
                                "content": char
                            }
                        }
                    ]
                }
                data_json = json.dumps(response_data)
                yield f'data: {data_json}\n\n'
                time.sleep(speed)  # 模拟延迟
            
            # 发送结束标记
            yield 'data: {"type": "end"}\n\n'
        
        # 返回流式响应
        return Response(stream_with_context(generate()), 
                       content_type='text/event-stream')
    
    except Exception as e:
        return {"success": False, "error": str(e)}, 500
