"""
加密模块
用于安全存储API密钥
"""
import os
from cryptography.fernet import Fernet
from shared.config import settings
# 自动加载.env
try:
    from dotenv import load_dotenv
    load_dotenv()
except ImportError:
    pass


# 获取或生成加密密钥
def get_encryption_key() -> bytes:
    """获取加密密钥"""
    # 优先从AI_ENCRYPTION_KEY环境变量获取
    key = os.environ.get("AI_ENCRYPTION_KEY")
    if not key:
        # 兼容旧变量名
        key = os.environ.get("ENCRYPTION_KEY")
    if not key:
        # 在开发环境，使用固定密钥（仅供开发使用）
        if getattr(settings, 'is_development', False):
            key = "Lz9kF8E5hG7vM3nB2xQ1aW6yU4iO0pAs="
        else:
            raise ValueError("AI_ENCRYPTION_KEY environment variable is required in production")
    
    # 确保密钥是有效的Fernet密钥
    if len(key) == 32:
        # 如果是32字节，转换为base64编码的44字符
        import base64
        key = base64.urlsafe_b64encode(key.encode()).decode()
    
    return key.encode()


# 创建加密器
_fernet = None

def get_fernet() -> Fernet:
    """获取Fernet加密器实例"""
    global _fernet
    if _fernet is None:
        _fernet = Fernet(get_encryption_key())
    return _fernet


def encrypt_api_key(api_key: str) -> str:
    """加密API密钥"""
    if not api_key:
        return ""
    
    fernet = get_fernet()
    encrypted = fernet.encrypt(api_key.encode())
    return encrypted.decode()


def decrypt_api_key(encrypted_key: str) -> str:
    """解密API密钥"""
    if not encrypted_key:
        return ""
    
    try:
        fernet = get_fernet()
        decrypted = fernet.decrypt(encrypted_key.encode())
        return decrypted.decode()
    except Exception as e:
        # 记录错误但不抛出，避免泄露信息
        import logging
        logging.error(f"Failed to decrypt API key: {e}")
        return ""


def generate_encryption_key() -> str:
    """生成新的加密密钥"""
    return Fernet.generate_key().decode()


# 开发环境工具函数
def test_encryption():
    """测试加密解密功能"""
    test_key = "sk-test-1234567890"
    
    encrypted = encrypt_api_key(test_key)
    print(f"Original: {test_key}")
    print(f"Encrypted: {encrypted}")
    
    decrypted = decrypt_api_key(encrypted)
    print(f"Decrypted: {decrypted}")
    
    assert test_key == decrypted, "Encryption/Decryption failed!"
    print("Encryption test passed!")


if __name__ == "__main__":
    # 生成新密钥或测试
    import sys
    if len(sys.argv) > 1 and sys.argv[1] == "generate":
        print(f"New encryption key: {generate_encryption_key()}")
    else:
        test_encryption() 