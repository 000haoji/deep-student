import React from 'react';
import ReactMarkdown from 'react-markdown';
import remarkGfm from 'remark-gfm';
import remarkMath from 'remark-math';
import rehypeKatex from 'rehype-katex';

interface MarkdownRendererProps {
  content: string;
  className?: string;
}

// 简化的LaTeX预处理，最小化干预
const preprocessLatex = (content: string): string => {
  if (!content) return '';
  
  let processedContent = content;

  // 专门处理 bmatrix 环境
  processedContent = processedContent.replace(/\\begin{bmatrix}(.*?)\\end{bmatrix}/gs, (match, matrixContent) => {
    // 移除每行末尾 \ 之前和之后的空格
    let cleanedMatrix = matrixContent.replace(/\s*\\\\\s*/g, ' \\\\ '); // 保留一个空格以便阅读，KaTeX应能处理
    // 移除 & 周围的空格
    cleanedMatrix = cleanedMatrix.replace(/\s*&\s*/g, '&');
    // 移除行首和行尾的空格
    cleanedMatrix = cleanedMatrix.split(' \\\\ ').map((row: string) => row.trim()).join(' \\\\ ');
    return `\\begin{bmatrix}${cleanedMatrix}\\end{bmatrix}`;
  });

  return processedContent;
};

export const MarkdownRenderer: React.FC<MarkdownRendererProps> = ({ 
  content, 
  className = '' 
}) => {
  // 预处理内容，处理LaTeX公式
  const processedContent = preprocessLatex(content);
  
  return (
    <div className={`markdown-content ${className}`}>
      <ReactMarkdown
        remarkPlugins={[remarkGfm, remarkMath]}
        rehypePlugins={[
          [rehypeKatex, {
            throwOnError: false, // 不抛出错误，以红色显示错误内容
            errorColor: '#cc0000',
            strict: false, // 宽松模式
            trust: false, // 安全模式
            macros: {
              '\\RR': '\\mathbb{R}',
              '\\NN': '\\mathbb{N}',
              '\\ZZ': '\\mathbb{Z}',
              '\\QQ': '\\mathbb{Q}',
              '\\CC': '\\mathbb{C}'
            }
          }]
        ]}
        components={{
          // 自定义代码块渲染
          code: ({ node, inline, className, children, ...props }: any) => {
            return !inline ? (
              <pre className="code-block">
                <code className={className} {...props}>
                  {children}
                </code>
              </pre>
            ) : (
              <code className="inline-code" {...props}>
                {children}
              </code>
            );
          },
          // 自定义表格渲染
          table: ({ children }) => (
            <div className="table-wrapper">
              <table className="markdown-table">{children}</table>
            </div>
          ),
        }}
      >
        {processedContent}
      </ReactMarkdown>
    </div>
  );
};