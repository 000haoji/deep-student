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
            return self._mock_stream_response(request_data, task_type)
        else:
            return await self._mock_single_response(request_data, task_type)
    
    def _build_messages(self, task_type: TaskType, content: Dict[str, Any]) -> list:
        """构建消息列表"""
        messages = []
        
        # 根据任务类型添加系统提示
        if task_type == TaskType.OCR:
            messages.append({
                "role": "system",
                "content": """You are an OCR expert. Extract text from images accurately.
If the image contains mathematical formulas, ensure they are correctly transcribed, preferably in LaTeX format where appropriate."""
            })
        elif task_type == TaskType.PROBLEM_ANALYSIS:
            problem_text = content.get("text", "")
            subject = content.get("subject", "通用") # Get subject if available
            user_answer = content.get("user_answer")
            correct_answer = content.get("correct_answer")

            system_prompt = f"""You are an expert educational assistant. Your task is to analyze a student's problem.
The problem is in the subject: {subject}.
Please provide a comprehensive analysis as a single JSON object with the following keys:
- "knowledge_points": (List[str]) A list of key knowledge points or concepts covered by this problem.
- "difficulty_level": (int) An estimated difficulty level from 1 (very easy) to 5 (very hard).
- "error_analysis": (str) An analysis of potential common errors students might make. If a student's answer is provided, analyze that specific error in relation to the correct answer. If no student answer is available, describe common pitfalls for this type of problem.
- "solution": (str) A step-by-step solution to the problem.
- "tags": (List[str]) Relevant tags for classifying this problem (e.g., "Calculus", "Integration by Parts", "Word Problem").
- "suggested_category": (str) Suggest a single, most appropriate category name for this problem within the given subject. For example, if subject is Math, category could be "Algebra", "Geometry", "Calculus", "Probability", etc. Be specific.

Ensure your entire response is a single, valid JSON object.
"""
            messages.append({
                "role": "system", 
                "content": system_prompt
            })
            
            user_message_parts = [f"Problem: {problem_text}"]
            if user_answer:
                user_message_parts.append(f"Student's Answer: {user_answer}")
            if correct_answer:
                user_message_parts.append(f"Correct Answer: {correct_answer}")
            
            messages.append({
                "role": "user",
                "content": "\n".join(user_message_parts)
            })
            # For PROBLEM_ANALYSIS, messages are fully constructed, so return early.
            return messages
        
        # 添加用户消息 (for other task types like generic prompt)
        if "prompt" in content: # Generic prompt
            messages.append({
                "role": "user",
                "content": content["prompt"]
            })
        elif "text" in content: # Fallback if structure is just text
             messages.append({
                "role": "user",
                "content": content["text"]
            })
        # If image_base64 is part of content for OCR, specific handling for that message structure might be needed here
        # For example, OpenAI vision model expects content as a list of dicts (text and image_url/base64)

        return messages
    
    async def _mock_single_response(self, request_data: Dict[str, Any], task_type: TaskType) -> Dict[str, Any]:
        """模拟单次响应"""
        if task_type == TaskType.PROBLEM_ANALYSIS:
            subject_from_prompt = "General" # Default
            if request_data and "messages" in request_data:
                for msg in request_data["messages"]:
                    if msg.get("role") == "system" and "subject:" in msg.get("content", ""):
                        try:
                            subject_from_prompt = msg["content"].split("subject:")[1].split("\n")[0].strip()
                            break
                        except IndexError:
                            pass # Keep default
            
            mock_analysis = {
                "knowledge_points": ["Mock Knowledge Point 1", "Mock Knowledge Point 2 for " + subject_from_prompt],
                "difficulty_level": 3,
                "error_analysis": "This is a mock error analysis. Students often forget to check for edge cases, especially in " + subject_from_prompt + ".",
                "solution": "Step 1: Understand the problem. Step 2: Apply mock formula for " + subject_from_prompt + ". Step 3: Get mock result.",
                "tags": ["Mock Tag", "Common Mistake", subject_from_prompt + " Tag"],
                "suggested_category": f"Mock Category for {subject_from_prompt}"
            }
            return {
                "content": json.dumps(mock_analysis), # Return as JSON string
                "usage": { "prompt_tokens": 50, "completion_tokens": 150, "total_tokens": 200 }
            }
        elif task_type == TaskType.OCR:
            # For OCR, content might be a dict like {"text": "extracted text"}
            return {
                "content": {"text": "Mock OCR result: Equation \(E=mc^2\). Some Chinese text: 模拟光学字符识别结果。"},
                "usage": { "prompt_tokens": 5, "completion_tokens": 25, "total_tokens": 30 }
            }
        else: # Default mock response
            return {
                "content": "这是一个模拟的AI响应。在实际使用中，这里会返回真实的API响应。",
                "usage": { "prompt_tokens": 10, "completion_tokens": 20, "total_tokens": 30 }
            }
    
    async def _mock_stream_response(self, request_data: Dict[str, Any], task_type: TaskType) -> AsyncGenerator[Dict[str, Any], None]:
        """模拟流式响应"""
        if task_type == TaskType.PROBLEM_ANALYSIS:
            # Get the complete mock JSON response for problem analysis
            mock_json_content_dict = await self._mock_single_response(request_data, task_type)
            full_json_str = mock_json_content_dict["content"] # This is already a JSON string
            
            # Simulate streaming the JSON string in parts
            chunk_size = 20  # Adjust chunk size as needed
            current_pos = 0
            idx = 0
            while current_pos < len(full_json_str):
                chunk_content = full_json_str[current_pos : current_pos + chunk_size]
                current_pos += chunk_size
                finished = current_pos >= len(full_json_str)
                
                # Yield a dictionary similar to what OpenAI streaming API might produce for chat completions
                # (delta content, finish reason, etc.)
                yield {
                    "choices": [{
                        "delta": {"content": chunk_content},
                        "index": 0, # Assuming one choice
                        "finish_reason": "stop" if finished else None
                    }],
                    "usage": mock_json_content_dict["usage"] if finished else None # Send usage with last chunk
                }
                idx +=1
            return

        # Default stream mock for other types (e.g., simple text streaming)
        chunks = ["这是", "一个", "模拟的", "流式", "响应。"]
        for i, chunk_text in enumerate(chunks):
            yield {
                 "choices": [{
                    "delta": {"content": chunk_text},
                    "index": 0,
                    "finish_reason": "stop" if i == len(chunks) - 1 else None
                }],
                "usage": {"completion_tokens": 5, "total_tokens": 5} if i == len(chunks) - 1 else None
            }
    
    async def check_health(self) -> bool:
        """检查健康状态"""
        # 实际应该发送测试请求
        return True
    
    def calculate_cost(self, usage: Dict[str, int]) -> float:
        """计算成本"""
        total_tokens = usage.get("total_tokens", 0)
        # GPT-4 定价示例：$0.03 per 1K tokens
        cost_per_1k = 0.03 # This should be model-specific from DB config
        return (total_tokens / 1000) * cost_per_1k
