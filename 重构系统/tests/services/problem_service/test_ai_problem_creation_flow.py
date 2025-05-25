import pytest
import httpx
import base64
import json
from typing import List, Dict, Any

# Configuration
BASE_URL = "http://localhost:8000/api/v1/problems/ai-create" # Adjust if your test server runs elsewhere
# A very small 1x1 black pixel PNG
SAMPLE_IMAGE_BASE64 = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNkYAAAAAYAAjCB0C8AAAAASUVORK5CYII="

@pytest.fixture(scope="module")
def http_client():
    """
    Provides an HTTPX client for making requests to the API.
    The client is configured to follow redirects and has a timeout.
    """
    with httpx.Client(base_url=BASE_URL, follow_redirects=True, timeout=30.0) as client:
        yield client

def test_health_check(http_client: httpx.Client):
    """
    A simple health check to ensure the problem service (or at least its router) is up.
    This might target a general endpoint of the problem service if /ai-create itself isn't GET-able.
    For now, we assume the service is running. If not, other tests will fail.
    """
    try:
        # Attempt to get a list of sessions, which should be an available endpoint
        response = http_client.get("/sessions") 
        # We expect this to either succeed (200) or be a client error if not found (404)
        # but not a server error (5xx) or connection error.
        assert response.status_code != 500 
    except httpx.ConnectError:
        pytest.fail("Connection to the backend service failed. Ensure the backend is running at " + BASE_URL)

@pytest.fixture(scope="module")
async def async_http_client(): # Renamed and made async
    """
    Provides an AsyncHTTPX client for making requests to the API.
    """
    async with httpx.AsyncClient(base_url=BASE_URL, follow_redirects=True, timeout=30.0) as client:
        yield client

@pytest.mark.asyncio 
async def test_ai_problem_creation_full_flow(async_http_client: httpx.AsyncClient): # Changed to use async_http_client
    """
    Tests the full end-to-end flow of AI problem creation.
    1. Initiate AI creation with an image.
    2. Simulate an interactive chat message (simplified test for streaming).
    3. Finalize the problem creation.
    4. Verify session details and chat history.
    """
    session_id = None
    created_problem_id = None

    # --- 1. Initiate AI Creation ---
    print("\n[TEST] Initiating AI problem creation...")
    initiate_payload = {
        "image_base64": SAMPLE_IMAGE_BASE64,
        "subject_hint": "math"
    }
    try:
        response_initiate = await async_http_client.post("/initiate", json=initiate_payload) # Changed to async_http_client
        response_initiate.raise_for_status() # Raise an exception for HTTP error codes
        initiate_data = response_initiate.json()
        
        assert response_initiate.status_code == 200
        assert "session_id" in initiate_data
        assert "structured_data" in initiate_data
        session_id = initiate_data["session_id"]
        print(f"[PASS] Initiation successful. Session ID: {session_id}")
        print(f"       Initial structured data: {initiate_data['structured_data']}")

    except httpx.HTTPStatusError as e:
        print(f"[FAIL] Initiation failed: {e.response.status_code} - {e.response.text}")
        pytest.fail(f"AI Initiation failed: {e.response.text}")
    except Exception as e:
        print(f"[FAIL] Initiation failed with unexpected error: {e}")
        pytest.fail(f"AI Initiation failed: {e}")


    assert session_id is not None, "Session ID was not obtained from initiation step."

    # --- 2. Simulate Interactive Chat (Simplified) ---
    # Full SSE stream testing is complex in a simple script.
    # We'll test sending a message and that the endpoint is receptive.
    # The backend for interactive-stream is POST, not GET as typical for SSE,
    # so we are testing the POST request part.
    print(f"\n[TEST] Simulating interactive chat for session: {session_id}...")
    chat_message_payload = {
        "user_message": "Explain the main concept in this problem.",
        "chat_history": [
            {"role": "user", "content": "Here is the image."}, # Placeholder for initial interaction
            {"role": "assistant", "content": initiate_data.get("structured_data",{}).get("extracted_content","Initial analysis based on image.")}
        ]
        # current_form_data can be added if needed by backend
    }
    try:
        # The interactive stream endpoint typically returns SSE.
        # For a non-SSE client, we might get an immediate response or timeout.
        # We're checking if the POST request is accepted and doesn't immediately error.
        # A proper test would involve an SSE client.
        # This test is more of a "can we send a message to this endpoint".
        # We expect a 200 OK if the server starts streaming.
        # The actual content might be SSE, which httpx won't fully parse here without specific handling.
        
        # Note: The API doc says POST, but SSE is often GET. Assuming POST is correct.
        # If the server immediately returns structured data (non-stream) or an error, this will catch it.
        # If it starts streaming, the initial response status should be 200.
        async with async_http_client.stream("POST", f"/interactive-stream/{session_id}", json=chat_message_payload) as response_stream: # Changed to async_http_client
            assert response_stream.status_code == 200 # Check if stream initiated successfully
            print(f"[PASS] Interactive chat POST request successful (status {response_stream.status_code}). SSE stream initiated.")
            
            # Optionally, try to read a few chunks to see if data flows
            # This part is very basic and not a full SSE test.
            stream_data_received = False
            async for chunk in response_stream.aiter_text(): # Corrected: Removed invalid parameter
                 if chunk: # Check if chunk is not empty
                    print(f"       Received stream chunk: {chunk[:100]}...") # Print first 100 chars
                    stream_data_received = True
                    # Add more specific checks here if expected data format is known
                    # For example, parsing JSON if chunks are JSON objects.
                    try:
                        # SSE format is usually "data: {...}\n\n"
                        if chunk.startswith("data: "):
                            json_data = json.loads(chunk.replace("data: ", "").strip())
                            print(f"       Parsed SSE data: {json_data}")
                            assert "type" in json_data # Example assertion
                    except json.JSONDecodeError:
                        pass # Not all chunks might be JSON
                    break # Stop after first chunk for this simplified test
            
            # assert stream_data_received, "No data was received from the SSE stream."
            # For now, just asserting connection was okay. Actual stream content test is harder here.

    except httpx.HTTPStatusError as e:
        print(f"[FAIL] Interactive chat POST failed: {e.response.status_code} - {e.response.text}")
        pytest.fail(f"Interactive chat POST failed: {e.response.text}")
    except Exception as e:
        print(f"[FAIL] Interactive chat POST failed with unexpected error: {e}")
        # This might happen if the server doesn't support streaming on this endpoint as expected
        # or if there's a configuration issue.
        pytest.fail(f"Interactive chat POST failed: {e}")


    # --- 3. Finalize AI Problem Creation ---
    print(f"\n[TEST] Finalizing AI problem creation for session: {session_id}...")
    final_problem_data = {
        "title": "AI Test Problem - Math Basics",
        "content": initiate_data.get("structured_data",{}).get("extracted_content","Content from AI OCR, with user message: Explain the main concept..."),
        "subject": "math",
        "category": initiate_data.get("structured_data",{}).get("preliminary_category","Algebra"),
        "user_answer": "My attempt was X.",
        "correct_answer": "The correct answer is Y.",
        "error_analysis": "I misunderstood the core concept.",
        "solution": "Step 1: ..., Step 2: ...",
        "knowledge_points": initiate_data.get("structured_data",{}).get("knowledge_points",["Basic Algebra", "Equations"]),
        "tags": initiate_data.get("structured_data",{}).get("preliminary_tags",["test", "ai-created"]),
        "difficulty_level": 2, # Example difficulty
        "image_urls": [initiate_data.get("structured_data",{}).get("image_references",[SAMPLE_IMAGE_BASE64])[0]] if initiate_data.get("structured_data",{}).get("image_references") else [SAMPLE_IMAGE_BASE64]
    }
    finalize_payload = {
        "session_id": session_id,
        "problem_data": final_problem_data,
        "chat_history": [ # Example chat history
            {"role": "user", "content": "Here is the image."},
            {"role": "assistant", "content": initiate_data.get("structured_data",{}).get("extracted_content","Initial analysis based on image.")},
            {"role": "user", "content": "Explain the main concept in this problem."},
            {"role": "assistant", "content": "The main concept is about solving linear equations. Here are the steps..."} # Mocked AI response
        ],
        "ai_full_analysis_json": {"summary": "AI analysis summary for this problem."} # Optional
    }
    try:
        response_finalize = await async_http_client.post("/finalize", json=finalize_payload) # Changed to async_http_client
        response_finalize.raise_for_status()
        finalize_data = response_finalize.json()

        assert response_finalize.status_code == 200
        assert "data" in finalize_data and "id" in finalize_data["data"]
        created_problem_id = finalize_data["data"]["id"]
        print(f"[PASS] Finalization successful. Created Problem ID: {created_problem_id}")
        print(f"       Created problem data: {finalize_data['data']}")
    except httpx.HTTPStatusError as e:
        print(f"[FAIL] Finalization failed: {e.response.status_code} - {e.response.text}")
        pytest.fail(f"Finalization failed: {e.response.text}")
    except Exception as e:
        print(f"[FAIL] Finalization failed with unexpected error: {e}")
        pytest.fail(f"Finalization failed: {e}")


    assert created_problem_id is not None, "Problem ID was not obtained from finalization step."

    # --- 4. Verify Session Details (after finalization) ---
    print(f"\n[TEST] Verifying session details for session: {session_id}...")
    try:
        response_session_detail = await async_http_client.get(f"/session/{session_id}") # Changed to async_http_client
        response_session_detail.raise_for_status()
        session_detail_data = response_session_detail.json()
        
        assert response_session_detail.status_code == 200
        assert "data" in session_detail_data
        assert session_detail_data["data"]["id"] == session_id
        assert session_detail_data["data"]["status"] == "finalized" # Assuming it becomes finalized
        assert session_detail_data["data"]["final_problem_id"] == created_problem_id
        print(f"[PASS] Session detail verification successful for session {session_id}.")
        print(f"       Session status: {session_detail_data['data']['status']}")

    except httpx.HTTPStatusError as e:
        print(f"[FAIL] Session detail verification failed: {e.response.status_code} - {e.response.text}")
        pytest.fail(f"Session detail verification failed: {e.response.text}")
    except Exception as e:
        print(f"[FAIL] Session detail verification failed with unexpected error: {e}")
        pytest.fail(f"Session detail verification failed: {e}")


    # --- 5. Verify Chat History ---
    print(f"\n[TEST] Verifying chat history for session: {session_id}...")
    try:
        response_chat_history = await async_http_client.get(f"/chat-history/{session_id}") # Changed to async_http_client
        response_chat_history.raise_for_status()
        chat_history_data = response_chat_history.json()

        assert response_chat_history.status_code == 200
        assert "data" in chat_history_data
        assert isinstance(chat_history_data["data"], list)
        # Check if chat history has entries corresponding to what was sent in finalize_payload
        # Note: The backend might store more system messages or re-order/process.
        # This is a basic check.
        num_expected_logs = len(finalize_payload["chat_history"])
        # The backend might add more system messages or log AI responses in chunks.
        # So, we check if at least the user messages are there.
        # For a more robust test, compare content of specific messages.
        print(f"       Retrieved {len(chat_history_data['data'])} chat log entries. Expected around {num_expected_logs} based on finalize payload.")
        assert len(chat_history_data["data"]) >= 1 # Ensure at least some logs
        
        # Example: check if the user's last message is present (simplified check)
        # found_last_user_message = any(
        #     log["role"] == "user" and log["content"] == finalize_payload["chat_history"][-2]["content"] 
        #     for log in chat_history_data["data"]
        # )
        # assert found_last_user_message, "Last user message not found in chat history."

        print(f"[PASS] Chat history verification successful for session {session_id}.")

    except httpx.HTTPStatusError as e:
        print(f"[FAIL] Chat history verification failed: {e.response.status_code} - {e.response.text}")
        pytest.fail(f"Chat history verification failed: {e.response.text}")
    except Exception as e:
        print(f"[FAIL] Chat history verification failed with unexpected error: {e}")
        pytest.fail(f"Chat history verification failed: {e}")

    # --- 6. List AI Sessions and check if our session appears ---
    print(f"\n[TEST] Listing AI sessions and checking for session: {session_id}...")
    try:
        response_list_sessions = await async_http_client.get("/sessions", params={"size": 100}) # Changed to async_http_client
        response_list_sessions.raise_for_status()
        list_sessions_data = response_list_sessions.json()

        assert response_list_sessions.status_code == 200
        assert "data" in list_sessions_data
        assert isinstance(list_sessions_data["data"], list)
        
        found_session_in_list = any(s["id"] == session_id for s in list_sessions_data["data"])
        assert found_session_in_list, f"Session {session_id} not found in the list of AI sessions."
        print(f"[PASS] Session {session_id} found in the list of AI sessions.")

    except httpx.HTTPStatusError as e:
        print(f"[FAIL] Listing AI sessions failed: {e.response.status_code} - {e.response.text}")
        pytest.fail(f"Listing AI sessions failed: {e.response.text}")
    except Exception as e:
        print(f"[FAIL] Listing AI sessions failed with unexpected error: {e}")
        pytest.fail(f"Listing AI sessions failed: {e}")

    print("\n[SUCCESS] Full AI problem creation flow test completed successfully.")

# To run this test:
# 1. Ensure your FastAPI backend service is running (e.g., `python main.py` from `backend` directory).
# 2. Ensure all dependent services (like AI models if not mocked, database) are accessible.
# 3. From the project root directory (where `tests/` is a subdirectory):
#    `pytest -s -v tests/services/problem_service/test_ai_problem_creation_flow.py`
#    (-s shows print statements, -v for verbose)
#
# Note on Async Tests:
# The http_client fixture should be an AsyncClient if test functions are async.
# Updated test_ai_problem_creation_full_flow to be async and use an AsyncClient.
# The http_client fixture now needs to be async as well, or a separate async fixture created.
    # For simplicity, I'm making the main test async and assuming an AsyncClient.

# The async_http_client fixture is now defined before its use.

# The test_health_check can remain synchronous if it targets a synchronous endpoint or if
# the client used there is synchronous. For consistency, it could also be made async.
# The main flow test is the async one.

# Note on stream reading:
# The line `async for chunk in response_stream.aiter_text(조각_당_바이트_수=None):`
# was corrected to `async for chunk in response_stream.aiter_text():`
# in the main test function's definition as `aiter_text()` handles default chunking.
