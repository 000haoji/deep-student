/**
 * 数据整合引擎 - 回顾分析功能的核心数据处理模块
 * 
 * 负责将多个错题的信息整合成统一的文本格式，供AI分析使用
 */

import { MistakeItem, MistakeConsolidationData, ConsolidatedMistakeData, ChatMessage } from '../types/index';

export class DataConsolidationEngine {
  /**
   * 从错题对象中提取关键信息用于整合
   */
  static extractMistakeData(mistake: MistakeItem): MistakeConsolidationData {
    return {
      mistakeId: mistake.id,
      ocr_text: mistake.ocr_text,
      user_question: mistake.user_question,
      chat_history: mistake.chat_history.filter(msg => 
        // 只保留用户和助手的消息内容，排除思维链
        msg.role === 'user' || msg.role === 'assistant'
      ).map(msg => ({
        role: msg.role,
        content: msg.content,
        timestamp: msg.timestamp,
        // 不包含thinking_content
      })),
    };
  }

  /**
   * 将多个错题信息整合成结构化的长文本
   */
  static consolidateMistakes(
    mistakes: MistakeItem[], 
    userOverallPrompt: string,
    consolidationOptions?: {
      includeImages?: boolean;
      includeChatHistory?: boolean;
      maxChatMessagesPerMistake?: number;
      includeTimestamps?: boolean;
      includeTags?: boolean;
      customTemplate?: string;
    }
  ): ConsolidatedMistakeData {
    const options = {
      includeImages: false, // 暂不支持图片整合
      includeChatHistory: true,
      maxChatMessagesPerMistake: 10,
      includeTimestamps: false,
      includeTags: true,
      ...consolidationOptions,
    };

    const selectedMistakes = mistakes.map(mistake => this.extractMistakeData(mistake));
    const consolidatedText = this.generateConsolidatedText(mistakes, options);

    return {
      selectedMistakes,
      consolidatedText,
      userOverallPrompt,
    };
  }

  /**
   * 生成整合后的文本内容
   */
  private static generateConsolidatedText(
    mistakes: MistakeItem[], 
    options: any
  ): string {
    let consolidatedText = '';
    
    // 添加整体说明
    consolidatedText += '# 错题回顾分析数据\n\n';
    consolidatedText += `本次回顾分析包含 ${mistakes.length} 道错题，以下是详细信息：\n\n`;
    
    mistakes.forEach((mistake, index) => {
      consolidatedText += this.formatSingleMistake(mistake, index + 1, options);
      consolidatedText += '\n';
    });

    return consolidatedText;
  }

  /**
   * 格式化单个错题的信息
   */
  private static formatSingleMistake(mistake: MistakeItem, index: number, options: any): string {
    let mistakeText = '';
    
    mistakeText += `## 错题 ${index} (ID: ${mistake.id})\n\n`;
    
    // 基本信息
    mistakeText += `**科目**: ${mistake.subject}\n`;
    mistakeText += `**错题类型**: ${mistake.mistake_type}\n`;
    
    if (options.includeTags && mistake.tags.length > 0) {
      mistakeText += `**相关标签**: ${mistake.tags.join(', ')}\n`;
    }
    
    if (options.includeTimestamps) {
      mistakeText += `**创建时间**: ${new Date(mistake.created_at).toLocaleString()}\n`;
    }
    
    mistakeText += '\n';
    
    // 题目内容
    mistakeText += '### 题目内容\n';
    mistakeText += '```\n';
    mistakeText += mistake.ocr_text || '(无题目文字内容)';
    mistakeText += '\n```\n\n';
    
    // 原始问题
    mistakeText += '### 我的原始问题\n';
    mistakeText += mistake.user_question;
    mistakeText += '\n\n';
    
    // 历史交流记录
    if (options.includeChatHistory && mistake.chat_history.length > 0) {
      mistakeText += '### 历史交流记录\n';
      
      const messages = mistake.chat_history
        .slice(0, options.maxChatMessagesPerMistake)
        .filter(msg => msg.role === 'user' || msg.role === 'assistant');
      
      messages.forEach((message, msgIndex) => {
        const roleDisplay = message.role === 'user' ? '👤 用户' : '🤖 助手';
        mistakeText += `**${roleDisplay}**: ${message.content}\n\n`;
      });
      
      if (mistake.chat_history.length > options.maxChatMessagesPerMistake) {
        mistakeText += `*(还有 ${mistake.chat_history.length - options.maxChatMessagesPerMistake} 条消息未显示)*\n\n`;
      }
    }
    
    mistakeText += '---\n\n';
    
    return mistakeText;
  }

  /**
   * 验证整合数据的有效性
   */
  static validateConsolidatedData(data: ConsolidatedMistakeData): {
    isValid: boolean;
    errors: string[];
    warnings: string[];
  } {
    const errors: string[] = [];
    const warnings: string[] = [];

    // 检查必要字段
    if (!data.userOverallPrompt || data.userOverallPrompt.trim().length === 0) {
      errors.push('用户总体分析指引不能为空');
    }

    if (!data.selectedMistakes || data.selectedMistakes.length === 0) {
      errors.push('至少需要选择一个错题');
    }

    if (!data.consolidatedText || data.consolidatedText.trim().length === 0) {
      errors.push('整合文本不能为空');
    }

    // 检查数据质量
    if (data.selectedMistakes) {
      data.selectedMistakes.forEach((mistake, index) => {
        if (!mistake.ocr_text || mistake.ocr_text.trim().length === 0) {
          warnings.push(`错题 ${index + 1} 没有题目文字内容`);
        }
        
        if (!mistake.user_question || mistake.user_question.trim().length === 0) {
          warnings.push(`错题 ${index + 1} 没有用户问题`);
        }
      });
    }

    // 检查整合文本长度
    if (data.consolidatedText && data.consolidatedText.length > 50000) {
      warnings.push('整合文本较长，可能超出某些AI模型的上下文限制');
    }

    return {
      isValid: errors.length === 0,
      errors,
      warnings,
    };
  }

  /**
   * 生成摘要信息
   */
  static generateSummary(data: ConsolidatedMistakeData): {
    mistakeCount: number;
    subjects: string[];
    totalChatMessages: number;
    averageOcrLength: number;
    consolidatedTextLength: number;
    estimatedTokens: number;
  } {
    const subjects = [...new Set(data.selectedMistakes.map(m => {
      // 从mistakeId中尝试获取科目信息，或使用其他方式
      return 'Unknown'; // 这里需要根据实际数据结构调整
    }))];

    const totalChatMessages = data.selectedMistakes.reduce(
      (sum, mistake) => sum + mistake.chat_history.length, 
      0
    );

    const totalOcrLength = data.selectedMistakes.reduce(
      (sum, mistake) => sum + mistake.ocr_text.length, 
      0
    );

    const averageOcrLength = data.selectedMistakes.length > 0 
      ? totalOcrLength / data.selectedMistakes.length 
      : 0;

    // 简单的token估算（1个token约等于4个字符）
    const estimatedTokens = Math.ceil(data.consolidatedText.length / 4);

    return {
      mistakeCount: data.selectedMistakes.length,
      subjects,
      totalChatMessages,
      averageOcrLength: Math.round(averageOcrLength),
      consolidatedTextLength: data.consolidatedText.length,
      estimatedTokens,
    };
  }

  /**
   * 预设的整合模板
   */
  static getPresetTemplates(): { [key: string]: string } {
    return {
      detailed: '详细模式 - 包含完整的聊天历史和所有信息',
      concise: '简洁模式 - 只包含题目内容和原始问题',
      focused: '重点模式 - 包含题目内容、问题和关键交流',
      comparative: '对比模式 - 突出不同错题间的关联和差异',
    };
  }

  /**
   * 根据模板生成特定格式的整合文本
   */
  static consolidateWithTemplate(
    mistakes: MistakeItem[],
    userOverallPrompt: string,
    templateType: 'detailed' | 'concise' | 'focused' | 'comparative'
  ): ConsolidatedMistakeData {
    const templateOptions = {
      detailed: {
        includeChatHistory: true,
        maxChatMessagesPerMistake: 20,
        includeTimestamps: true,
        includeTags: true,
      },
      concise: {
        includeChatHistory: false,
        maxChatMessagesPerMistake: 0,
        includeTimestamps: false,
        includeTags: false,
      },
      focused: {
        includeChatHistory: true,
        maxChatMessagesPerMistake: 5,
        includeTimestamps: false,
        includeTags: true,
      },
      comparative: {
        includeChatHistory: true,
        maxChatMessagesPerMistake: 3,
        includeTimestamps: false,
        includeTags: true,
      },
    };

    return this.consolidateMistakes(mistakes, userOverallPrompt, templateOptions[templateType]);
  }

  /**
   * 检查是否超出模型上下文限制
   */
  static checkContextLimits(
    data: ConsolidatedMistakeData,
    modelContextLimit: number = 32000 // 默认32k tokens
  ): {
    withinLimit: boolean;
    estimatedTokens: number;
    recommendedActions: string[];
  } {
    const estimatedTokens = Math.ceil(data.consolidatedText.length / 4);
    const withinLimit = estimatedTokens <= modelContextLimit * 0.8; // 保留20%缓冲
    
    const recommendedActions: string[] = [];
    
    if (!withinLimit) {
      recommendedActions.push('减少选择的错题数量');
      recommendedActions.push('使用简洁模式整合');
      recommendedActions.push('减少聊天历史记录的包含量');
      recommendedActions.push('考虑分批进行回顾分析');
    }

    return {
      withinLimit,
      estimatedTokens,
      recommendedActions,
    };
  }
}

export default DataConsolidationEngine;