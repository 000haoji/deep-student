import React, { useState, useEffect, useMemo } from 'react';
import { MarkdownRenderer } from './MarkdownRenderer';

interface StreamingMarkdownRendererProps {
  content: string;
  isStreaming: boolean;
  chainOfThought?: {
    enabled: boolean;
    details?: any;
  };
}

type ParsedContent = {
  thinkingContent: string;
  mainContent: string;
}

// 流式内容预处理函数
const preprocessStreamingContent = (content: string, isStreaming: boolean) => {
  if (!content) return { content: '', hasPartialMath: false };
  
  let processed = content;
  let hasPartialMath = false;
  
  // 检测不完整的数学公式
  const incompletePatterns = [
    /\$[^$]*$/,  // 以$结尾但没有关闭$
    /\$\$[^$]*$/,  // 以$$结尾但没有关闭$$
    /\\begin\{[^}]*\}[^\\]*$/,  // 不完整的环境
    /\\[a-zA-Z]+\{[^}]*$/,  // 不完整的命令
  ];
  
  if (isStreaming) {
    // 更精确地检测不完整的数学公式
    hasPartialMath = incompletePatterns.some(pattern => pattern.test(processed));
    
    // 不要隐藏不完整的公式，而是保持原样并添加指示符
    // 让用户看到正在输入的数学内容，即使还不完整
    if (hasPartialMath) {
      // 检查是否是真正的不完整公式（而不是正常的LaTeX语法）
      const hasOpenMath = (processed.match(/\$/g) || []).length % 2 !== 0;
      const hasOpenDisplayMath = (processed.match(/\$\$/g) || []).length % 2 !== 0;
      
      // 只有当确实存在未闭合的数学公式时才标记为不完整
      if (hasOpenMath || hasOpenDisplayMath) {
        // 保持内容不变，只是标记状态
        hasPartialMath = true;
      } else {
        hasPartialMath = false;
      }
    }
  }
  
  return { content: processed, hasPartialMath };
};

export const StreamingMarkdownRenderer: React.FC<StreamingMarkdownRendererProps> = ({ 
  content, 
  isStreaming
}) => {
  const [displayContent, setDisplayContent] = useState('');
  const [showCursor, setShowCursor] = useState(true);
  const [isPartialMath, setIsPartialMath] = useState(false);

  useEffect(() => {
    // 对流式内容进行智能处理
    const processedContent = preprocessStreamingContent(content, isStreaming);
    setDisplayContent(processedContent.content);
    setIsPartialMath(processedContent.hasPartialMath);
  }, [content, isStreaming]);

  useEffect(() => {
    if (isStreaming) {
      const interval = setInterval(() => {
        setShowCursor(prev => !prev);
      }, 500);
      return () => clearInterval(interval);
    } else {
      setShowCursor(false);
    }
  }, [isStreaming]);

  // 解析思维链内容
  const parseChainOfThought = (content: string): ParsedContent | null => {
    // 检查是否包含 <thinking> 标签
    const thinkingMatch = content.match(/<thinking>([\s\S]*?)<\/thinking>\s*/);
    
    if (thinkingMatch) {
      const thinkingContent = thinkingMatch[1].trim();
      const mainContent = content.replace(thinkingMatch[0], '').trim();
      
      return {
        thinkingContent,
        mainContent
      };
    }
    
    return null;
  };

  const parsedContent = parseChainOfThought(displayContent);

  // 使用 useMemo 优化性能
  const renderedContent = useMemo(() => {
    if (!displayContent) return null;
    return <MarkdownRenderer content={displayContent} />;
  }, [displayContent]);
  
  return (
    <div className="streaming-markdown">
      {parsedContent ? (
        <>
          {/* 渲染思维链内容 */}
          {parsedContent.thinkingContent && (
            <div className="chain-of-thought">
              <div className="chain-header">
                <span className="chain-icon">🧠</span>
                <span className="chain-title">AI 思考过程</span>
              </div>
              <div className="thinking-content">
                <MarkdownRenderer content={parsedContent.thinkingContent} />
              </div>
            </div>
          )}
          
          {/* 渲染主要内容 */}
          <div className="main-content">
            {parsedContent.mainContent ? (
              <MarkdownRenderer content={parsedContent.mainContent} />
            ) : (
              renderedContent
            )}
            {isStreaming && showCursor && (
              <span className="streaming-cursor">▋</span>
            )}
            {isPartialMath && isStreaming && (
              <span className="partial-math-indicator" title="正在输入数学公式...">📝</span>
            )}
          </div>
        </>
      ) : (
        <div className="normal-content">
          {renderedContent}
          {isStreaming && showCursor && (
            <span className="streaming-cursor">▋</span>
          )}
          {isPartialMath && isStreaming && (
            <span className="partial-math-indicator" title="正在输入数学公式...">📝</span>
          )}
        </div>
      )}
    </div>
  );
}; 