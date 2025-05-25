"""
OpenAI API提供商实现
"""
import json
import aiohttp
from typing import Dict, Any, Optional, AsyncGenerator, Tuple, List, Union

from .base import BaseAIProvider
from ..schemas import AIRequestSchema # Updated import
from ..models import TaskType, AIProvider as AIProviderEnum # For typing

# Default OpenAI API URL, can be overridden by config
DEFAULT_OPENAI_API_URL = "https://api.openai.com/v1/chat/completions"

class OpenAIProvider(BaseAIProvider):
    """OpenAI API提供商"""

    def __init__(
        self,
        model_name: str,
        provider_name: AIProviderEnum, # Should be AIProviderEnum.OPENAI
        api_key: Optional[str],
        base_url: Optional[str] = None, # Overrides DEFAULT_OPENAI_API_URL if provided
        timeout_seconds: int = 120,
        max_retries: int = 2,
        rpm_limit: Optional[int] = None, # For future rate limiting logic
        tpm_limit: Optional[int] = None, # For future rate limiting logic
        custom_headers: Optional[Dict[str, str]] = None,
        max_tokens_limit: Optional[int] = None,
    ):
        super().__init__(
            model_name=model_name,
            provider_name=provider_name,
            api_key=api_key,
            base_url=base_url or DEFAULT_OPENAI_API_URL, # Use default if not provided
            timeout_seconds=timeout_seconds,
            max_retries=max_retries,
            rpm_limit=rpm_limit,
            tpm_limit=tpm_limit,
            custom_headers=custom_headers,
            max_tokens_limit=max_tokens_limit
        )
        # It's good practice to have one session per provider instance for connection pooling
        self._session: Optional[aiohttp.ClientSession] = None

    async def _get_session(self) -> aiohttp.ClientSession:
        """获取或创建 aiohttp.ClientSession"""
        if self._session is None or self._session.closed:
            # You might want to configure connector limits or other session parameters here
            timeout = aiohttp.ClientTimeout(total=self.timeout_seconds)
            self._session = aiohttp.ClientSession(timeout=timeout, headers=self.custom_headers)
        return self._session

    async def close(self) -> None:
        """关闭 aiohttp.ClientSession"""
        if self._session and not self._session.closed:
            await self._session.close()
            self._session = None

    def _build_messages(self, request: AIRequestSchema) -> List[Dict[str, Any]]:
        """根据 AIRequestSchema 构建 OpenAI messages 列表"""
        messages: List[Dict[str, Any]] = []
        task_type = request.task_type
        context = request.context or {}
        
        # 1. System Prompt Construction
        system_content = "You are a helpful AI assistant." # Default
        if task_type == TaskType.OCR:
            system_content = "You are an OCR expert. Extract text from images accurately. If the image contains mathematical formulas, ensure they are correctly transcribed, preferably in LaTeX format where appropriate."
        elif task_type == TaskType.PROBLEM_ANALYSIS:
            subject = context.get("subject", "通用")
            system_content = f"""You are an expert educational assistant. Your task is to analyze a student's problem.
The problem is in the subject: {subject}.
Please provide a comprehensive analysis as a single JSON object with the following keys:
- "knowledge_points": (List[str]) A list of key knowledge points or concepts covered by this problem.
- "difficulty_level": (int) An estimated difficulty level from 1 (very easy) to 5 (very hard).
- "error_analysis": (str) An analysis of potential common errors students might make. If a student's answer is provided, analyze that specific error in relation to the correct answer. If no student answer is available, describe common pitfalls for this type of problem.
- "solution": (str) A step-by-step solution to the problem.
- "tags": (List[str]) Relevant tags for classifying this problem (e.g., "Calculus", "Integration by Parts", "Word Problem").
- "suggested_category": (str) Suggest a single, most appropriate category name for this problem within the given subject. For example, if subject is Math, category could be "Algebra", "Geometry", "Calculus", "Probability", etc. Be specific.
Ensure your entire response is a single, valid JSON object. Do not include any text outside this JSON object, including explanations or markdown formatting.
"""
        elif task_type == TaskType.PROBLEM_IMAGE_TO_STRUCTURED_JSON:
            system_content = """You are an intelligent OCR and problem analysis assistant.
Given an image of a problem, your tasks are:
1. Perform OCR to extract all text.
2. Analyze the extracted text and the image to understand the problem.
3. Output a single, valid JSON object containing the following fields:
    - "session_id": (str) placeholder, will be filled by system.
    - "raw_ocr_text": (str) The raw text extracted by OCR from the image.
    - "extracted_content": (str) The main content of the problem, cleaned and formatted.
    - "suggested_subject": (str enum: "math", "physics", "chemistry", "biology", "history", "geo", "english", "politics", "chinese", "other") AI suggested subject based on content. If a subject_hint is provided by the user, prioritize it if relevant, otherwise make your best guess.
    - "preliminary_category": (str) A preliminary, more specific category for the problem (e.g., "Algebra", "Optics", "Organic Chemistry").
    - "preliminary_tags": (List[str]) A list of 2-5 preliminary tags relevant to the problem.
    - "image_regions_of_interest": (Optional[List[Dict[str, Any]]]) Keep this as an empty list [] or null if not implemented.
    - "detected_language": (Optional[str]) e.g., "en", "zh-CN".
    - "original_image_ref": (str) placeholder, will be filled by system.
Ensure the output is ONLY the JSON object. No explanations or markdown formatting.
"""
        elif task_type == TaskType.PROBLEM_INTERACTIVE_DEEP_ANALYSIS:
            system_content = """You are an expert AI assistant helping a user refine a problem description and analyze it deeply.
The user will provide initial structured data about a problem (possibly from an image OCR) and then ask questions or give instructions.
Your goal is to interactively help the user:
- Clarify and complete all fields of the problem (title, content, subject, category, tags, knowledge points, difficulty, solution, error analysis, etc.).
- Answer questions about the problem, its concepts, or related topics.
- Provide suggestions for improvement or alternative approaches.
Maintain a helpful, analytical, and conversational tone. Be ready to update or suggest modifications to the problem's structured data based on the conversation.
The final output of this entire interactive process (managed by the user application) will be a complete problem record. Your responses in this stream should be conversational and informative chunks that contribute to this goal.
If the user asks for a JSON representation of the current state of the problem, provide it.
"""
        if system_content:
            messages.append({"role": "system", "content": system_content})

        # 2. History (if provided)
        if request.history:
            for entry in request.history:
                # Ensure role is 'user' or 'assistant'. Map 'ai' to 'assistant'.
                role = entry.get("role", "user")
                if role == "ai":
                    role = "assistant"
                messages.append({"role": role, "content": entry.get("content", "")})
        
        # 3. Current User Message / Prompt / Context
        user_message_parts = []
        # Text part
        if request.prompt:
            user_message_parts.append({"type": "text", "text": request.prompt})
        elif task_type == TaskType.OCR: # For OCR, if no prompt, default to "Extract text"
             user_message_parts.append({"type": "text", "text": context.get("ocr_prompt", "Extract all text from this image.")})
        elif task_type == TaskType.PROBLEM_ANALYSIS:
            problem_text = context.get("text", "")
            user_answer = context.get("user_answer")
            correct_answer = context.get("correct_answer")
            analysis_prompt_parts = [f"Problem: {problem_text}"]
            if user_answer: analysis_prompt_parts.append(f"Student's Answer: {user_answer}")
            if correct_answer: analysis_prompt_parts.append(f"Correct Answer: {correct_answer}")
            user_message_parts.append({"type": "text", "text": "\n".join(analysis_prompt_parts)})

        # Image part (common for several tasks)
        image_content = None
        if request.image_base64:
            # Basic type inference, could be more sophisticated
            image_type = "image/png" # Default
            if "jpeg" in request.image_base64[:30].lower() or "jpg" in request.image_base64[:30].lower():
                image_type = "image/jpeg"
            image_content = {"type": "image_url", "image_url": {"url": f"data:{image_type};base64,{request.image_base64}"}}
        elif request.image_url:
            image_content = {"type": "image_url", "image_url": {"url": str(request.image_url)}}
        
        if image_content:
            if not user_message_parts or user_message_parts[0].get("type") != "text":
                # If only image, or to ensure text part exists for multi-modal if prompt was empty
                user_message_parts.insert(0, {"type": "text", "text": "Analyze the provided image."}) # Default text for image
            user_message_parts.append(image_content)

        if user_message_parts:
            # OpenAI expects 'content' to be a string for single text, or a list for multi-part.
            if len(user_message_parts) == 1 and user_message_parts[0]["type"] == "text":
                messages.append({"role": "user", "content": user_message_parts[0]["text"]})
            else:
                messages.append({"role": "user", "content": user_message_parts})
        
        if not messages: # Fallback if everything else failed to produce messages
            messages.append({"role": "user", "content": "Hello."})
            
        return messages

    async def execute_task(self, request: AIRequestSchema) -> Dict[str, Any]:
        if not self.api_key:
            return {"error": "API key not configured for OpenAI provider.", "error_type": "config_error"}

        messages = self._build_messages(request)
        payload: Dict[str, Any] = {
            "model": self.model_name,
            "messages": messages,
            "temperature": request.temperature if request.temperature is not None else 0.7,
            "stream": False # Explicitly false for this method
        }
        if request.max_output_tokens:
            payload["max_tokens"] = request.max_output_tokens
        if request.output_format == "json_object" and (
            task_type == TaskType.PROBLEM_ANALYSIS or 
            task_type == TaskType.PROBLEM_IMAGE_TO_STRUCTURED_JSON
        ): # Only for specific models/tasks that support JSON mode reliably
             if "gpt-4-1106-preview" in self.model_name or "gpt-3.5-turbo-1106" in self.model_name or "gpt-4o" in self.model_name: # Check model compatibility
                payload["response_format"] = {"type": "json_object"}


        headers = {
            "Authorization": f"Bearer {self.api_key}",
            "Content-Type": "application/json"
        }
        if self.custom_headers:
            headers.update(self.custom_headers)

        session = await self._get_session()
        retries = 0
        last_exception = None

        while retries <= self.max_retries:
            try:
                async with session.post(self.base_url, json=payload, headers=headers) as response:
                    response_data = await response.json()
                    if response.status == 200:
                        choice = response_data.get("choices", [{}])[0]
                        message_content = choice.get("message", {}).get("content")
                        
                        result_text = None
                        result_json = None

                        if request.output_format == "json_object" or \
                           task_type == TaskType.PROBLEM_ANALYSIS or \
                           task_type == TaskType.PROBLEM_IMAGE_TO_STRUCTURED_JSON:
                            try:
                                result_json = json.loads(message_content)
                                result_text = message_content # Keep raw JSON string as text too
                            except json.JSONDecodeError:
                                # If JSON parsing fails, return it as text
                                result_text = message_content
                                # Optionally log this as a provider-side format error
                        else:
                            result_text = message_content
                        
                        return {
                            "content_text": result_text,
                            "content_json": result_json,
                            "usage": response_data.get("usage", {}), # Should contain prompt_tokens, completion_tokens
                            "error": None,
                            "error_type": None
                        }
                    else:
                        error_info = response_data.get("error", {})
                        error_message = error_info.get("message", "Unknown API error")
                        error_type = error_info.get("type", "api_error")
                        if response.status == 429: error_type = "rate_limit_error"
                        elif response.status == 401: error_type = "authentication_error"
                        elif response.status == 400: error_type = "invalid_request_error"
                        
                        last_exception = Exception(f"API Error {response.status}: {error_message} (Type: {error_type})")
                        # For 429, could implement specific backoff if not handled by aiohttp retries or a wrapper
                        # For now, just retry
                        
            except aiohttp.ClientError as e: # Covers network issues, timeouts
                last_exception = e
            
            retries += 1
            if retries <= self.max_retries:
                # Simple backoff, could be exponential
                await asyncio.sleep(1 * retries) 
            else: # Max retries reached
                error_msg_final = str(last_exception) if last_exception else "Max retries reached with unknown error."
                err_type_final = "network_error" if isinstance(last_exception, aiohttp.ClientError) else "api_error"
                return { "error": error_msg_final, "error_type": err_type_final }
        
        return { "error": "Max retries reached.", "error_type": "max_retries_exceeded" } # Should not be reached if loop breaks earlier


    async def execute_task_stream(
        self, request: AIRequestSchema
    ) -> AsyncGenerator[Union[str, Dict[str, Any]], None]:
        if not self.api_key:
            yield {"event": "error", "data": {"message": "API key not configured for OpenAI provider.", "type": "config_error"}}
            return

        messages = self._build_messages(request)
        payload: Dict[str, Any] = {
            "model": self.model_name,
            "messages": messages,
            "temperature": request.temperature if request.temperature is not None else 0.7,
            "stream": True
        }
        if request.max_output_tokens:
            payload["max_tokens"] = request.max_output_tokens
        # JSON mode typically not used with streaming in the same way, system prompt guides format.

        headers = {
            "Authorization": f"Bearer {self.api_key}",
            "Content-Type": "application/json",
            "Accept": "text/event-stream" 
        }
        if self.custom_headers:
            headers.update(self.custom_headers)
        
        session = await self._get_session()
        # Streaming typically doesn't retry in the same way due to partial data.
        # Service layer might need to handle re-initiation of stream if critical.
        try:
            async with session.post(self.base_url, json=payload, headers=headers) as response:
                if response.status != 200:
                    error_data = await response.json()
                    error_info = error_data.get("error", {})
                    error_message = error_info.get("message", "Unknown API streaming error")
                    error_type = error_info.get("type", "api_error")
                    if response.status == 429: error_type = "rate_limit_error"
                    yield {"event": "error", "data": {"message": f"API Error {response.status}: {error_message}", "type": error_type}}
                    return

                async for line in response.content:
                    line_str = line.decode('utf-8').strip()
                    if line_str.startswith("data:"):
                        data_content = line_str[len("data:"):].strip()
                        if data_content == "[DONE]":
                            # yield {"event": "done"} # Signal stream completion
                            break 
                        try:
                            chunk_json = json.loads(data_content)
                            delta = chunk_json.get("choices", [{}])[0].get("delta", {})
                            content_chunk = delta.get("content")
                            if content_chunk: # If there's new text content
                                yield content_chunk 
                            
                            # Potentially yield usage at the end if OpenAI includes it in the last chunk
                            # or as a separate event in some future API version.
                            # For now, service.py calculates usage after stream ends.
                            finish_reason = chunk_json.get("choices", [{}])[0].get("finish_reason")
                            if finish_reason == "stop" or finish_reason == "length":
                                final_usage = chunk_json.get("usage") # OpenAI might include full usage in the last event with finish_reason
                                if final_usage:
                                     yield {"event": "usage", "data": final_usage} # Forward to service if available
                                break # Stream ended by API
                        except json.JSONDecodeError:
                            # Malformed chunk, log and optionally yield error or skip
                            print(f"Warning: Could not decode stream chunk: {data_content}") # Replace with self.log_warning
                            pass 
        except aiohttp.ClientError as e:
            yield {"event": "error", "data": {"message": f"Streaming ClientError: {str(e)}", "type": "network_error"}}
        except Exception as e: # Catch-all for other unexpected errors during streaming
             yield {"event": "error", "data": {"message": f"Unexpected streaming error: {str(e)}", "type": "unknown_stream_error"}}


    async def check_health(self) -> Tuple[bool, Optional[str]]:
        """检查OpenAI API健康状态"""
        if not self.api_key:
            return False, "API key not configured for OpenAI provider."

        # A lightweight call, e.g., listing models (first few) or a very short completion.
        # Using a short completion might be better as it also checks generation capability.
        health_check_payload = {
            "model": self.model_name if "gpt-3.5-turbo-instruct" not in self.model_name else "gpt-3.5-turbo", # instruct models are different endpoint
            "messages": [{"role": "user", "content": "Say 'Hello'."}],
            "max_tokens": 5,
            "temperature": 0.1
        }
        if "gpt-3.5-turbo-instruct" in self.model_name: # older completion endpoint
             del health_check_payload["messages"]
             health_check_payload["prompt"] = "Say 'Hello'."
             health_url = self.base_url.replace("/chat/completions", "/completions") if "/chat/completions" in self.base_url else self.base_url
        else:
            health_url = self.base_url

        headers = {
            "Authorization": f"Bearer {self.api_key}",
            "Content-Type": "application/json"
        }
        if self.custom_headers:
            headers.update(self.custom_headers)

        session = await self._get_session()
        try:
            async with session.post(health_url, json=health_check_payload, headers=headers) as response:
                if response.status == 200:
                    await response.json() # Consume response
                    return True, f"Successfully connected to OpenAI ({self.model_name})."
                else:
                    error_data = await response.json()
                    error_msg = error_data.get("error", {}).get("message", "Unknown health check error")
                    return False, f"OpenAI health check failed ({self.model_name}): {response.status} - {error_msg}"
        except aiohttp.ClientError as e:
            return False, f"OpenAI health check connection error ({self.model_name}): {str(e)}"
        except Exception as e:
            return False, f"OpenAI health check unexpected error ({self.model_name}): {str(e)}"
        # Ensure session is closed if this provider instance is short-lived
        # await self.close() # Or manage session lifecycle at a higher level

# Need to import asyncio for the sleep in execute_task retry logic
import asyncio
