"""
OpenAI API提供商实现
"""
from typing import Dict, Any, Optional, AsyncGenerator
import aiohttp
import json

from .base import BaseAIProvider
from ..models import TaskType


class OpenAIProvider(BaseAIProvider):
    """OpenAI API提供商"""
    
    async def call_api(
        self,
        task_type: TaskType,
        content: Dict[str, Any],
        stream: bool = False,
        max_tokens: Optional[int] = None,
        temperature: float = 0.7,
        timeout: Optional[int] = None,
        **kwargs
    ) -> Dict[str, Any] | AsyncGenerator[Dict[str, Any], None]:
        """调用OpenAI API"""
        # 构建请求数据
        messages = self._build_messages(task_type, content)
        
        request_data = {
            "model": self.model_name,
            "messages": messages,
            "temperature": temperature,
            "stream": stream
        }
        
        if max_tokens:
            request_data["max_tokens"] = max_tokens
        
        # 设置请求头
        headers = {
            "Authorization": f"Bearer {self.api_key}",
            "Content-Type": "application/json"
        }
        
        # 模拟API响应（实际应该调用真实API）
        if stream:
            return self._mock_stream_response(request_data)
        else:
            return await self._mock_single_response(request_data)
    
    def _build_messages(self, task_type: TaskType, content: Dict[str, Any]) -> list:
        """构建消息列表"""
        messages = []
        
        # 根据任务类型添加系统提示
        if task_type == TaskType.OCR:
            messages.append({
                "role": "system",
                "content": "You are an OCR expert. Extract text from images accurately."
            })
        elif task_type == TaskType.PROBLEM_ANALYSIS:
            messages.append({
                "role": "system", 
                "content": "You are an education expert. Analyze the student's errors and provide learning suggestions."
            })
        
        # 添加用户消息
        if "prompt" in content:
            messages.append({
                "role": "user",
                "content": content["prompt"]
            })
        elif "text" in content:
            messages.append({
                "role": "user",
                "content": content["text"]
            })
        
        return messages
    
    async def _mock_single_response(self, request_data: Dict[str, Any]) -> Dict[str, Any]:
        """模拟单次响应"""
        return {
            "content": "这是一个模拟的AI响应。在实际使用中，这里会返回真实的API响应。",
            "usage": {
                "prompt_tokens": 10,
                "completion_tokens": 20,
                "total_tokens": 30
            }
        }
    
    async def _mock_stream_response(self, request_data: Dict[str, Any]) -> AsyncGenerator[Dict[str, Any], None]:
        """模拟流式响应"""
        chunks = ["这是", "一个", "模拟的", "流式", "响应。"]
        
        for i, chunk in enumerate(chunks):
            yield {
                "content": chunk,
                "index": i,
                "finished": i == len(chunks) - 1
            }
    
    async def check_health(self) -> bool:
        """检查健康状态"""
        # 实际应该发送测试请求
        return True
    
    def calculate_cost(self, usage: Dict[str, int]) -> float:
        """计算成本"""
        total_tokens = usage.get("total_tokens", 0)
        # GPT-4 定价示例：$0.03 per 1K tokens
        cost_per_1k = 0.03
        return (total_tokens / 1000) * cost_per_1k 