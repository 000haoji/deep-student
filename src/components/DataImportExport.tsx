import React, { useState } from 'react';
import { TauriAPI } from '../utils/tauriApi';
import { DataStats } from './DataStats';
import { Upload, Download, BarChart3, AlertTriangle, Trash2, Lightbulb, FileText, Calendar, HardDrive, Settings } from 'lucide-react';

interface DataImportExportProps {
  onClose?: () => void;
}

export const DataImportExport: React.FC<DataImportExportProps> = ({ onClose }) => {
  const [isExporting, setIsExporting] = useState(false);
  const [isImporting, setIsImporting] = useState(false);
  const [importFile, setImportFile] = useState<File | null>(null);
  const [exportFormat, setExportFormat] = useState<'json' | 'csv'>('json');
  const [isExportingSettings, setIsExportingSettings] = useState(false);
  const [isImportingSettings, setIsImportingSettings] = useState(false);
  const [settingsImportFile, setSettingsImportFile] = useState<File | null>(null);

  // 导出数据
  const handleExport = async () => {
    setIsExporting(true);
    try {
      console.log('开始导出完整数据...');
      
      // 并行获取所有数据
      const [
        rawMistakes,
        statistics,
        apiConfigs,
        modelAssignments,
        subjectConfigs
      ] = await Promise.all([
        TauriAPI.getMistakes(),
        TauriAPI.getStatistics(),
        TauriAPI.getApiConfigurations(),
        TauriAPI.getModelAssignments(),
        TauriAPI.getAllSubjectConfigs(false) // 包括禁用的配置
      ]);

      // 新增：将图片文件转换为Base64包含在备份中
      console.log('开始处理图片备份，错题数量:', rawMistakes.length);
      const mistakesWithImages = await Promise.all(
        rawMistakes.map(async (mistake) => {
          try {
            // 处理问题图片
            const questionImageData = await Promise.all(
              mistake.question_images.map(async (imagePath) => {
                try {
                  const base64Data = await TauriAPI.getImageAsBase64(imagePath);
                  return {
                    path: imagePath,
                    data: base64Data
                  };
                } catch (error) {
                  console.warn(`备份图片失败: ${imagePath}`, error);
                  return {
                    path: imagePath,
                    data: null
                  };
                }
              })
            );

            // 处理解析图片
            const analysisImageData = await Promise.all(
              mistake.analysis_images.map(async (imagePath) => {
                try {
                  const base64Data = await TauriAPI.getImageAsBase64(imagePath);
                  return {
                    path: imagePath,
                    data: base64Data
                  };
                } catch (error) {
                  console.warn(`备份解析图片失败: ${imagePath}`, error);
                  return {
                    path: imagePath,
                    data: null
                  };
                }
              })
            );

            return {
              ...mistake,
              question_image_data: questionImageData,
              analysis_image_data: analysisImageData
            };
          } catch (error) {
            console.warn(`处理错题图片失败: ${mistake.id}`, error);
            return mistake;
          }
        })
      );

      console.log('图片备份处理完成');
      const mistakes = mistakesWithImages;

      // 获取系统设置（尝试获取常见设置键）
      const settingKeys = [
        'theme', 'language', 'auto_save', 'default_subject',
        'api_configs', 'model_assignments', 'model_adapter_options'
      ];
      
      const settings: Record<string, any> = {};
      for (const key of settingKeys) {
        try {
          const value = await TauriAPI.getSetting(key);
          if (value !== null) {
            settings[key] = value;
          }
        } catch (error) {
          console.warn(`获取设置失败 ${key}:`, error);
        }
      }
      
      const exportData = {
        version: '2.0', // 升级版本号以支持完整数据
        timestamp: new Date().toISOString(),
        data: {
          mistakes,
          reviews: [], // 目前回顾数据存储在错题聊天记录中
          settings: {
            system_settings: settings,
            api_configurations: apiConfigs,
            model_assignments: modelAssignments,
            subject_configurations: subjectConfigs
          },
          statistics
        }
      };

      // 统计图片备份信息
      const imageStats = mistakes.reduce((stats: any, mistake: any) => {
        stats.totalQuestionImages += mistake.question_images?.length || 0;
        stats.totalAnalysisImages += mistake.analysis_images?.length || 0;
        stats.successfulQuestionImages += mistake.question_image_data?.filter((img: any) => img.data).length || 0;
        stats.successfulAnalysisImages += mistake.analysis_image_data?.filter((img: any) => img.data).length || 0;
        return stats;
      }, {
        totalQuestionImages: 0,
        totalAnalysisImages: 0,
        successfulQuestionImages: 0,
        successfulAnalysisImages: 0
      });

      console.log('导出数据统计:', {
        错题数量: mistakes.length,
        系统设置: Object.keys(settings).length,
        API配置: apiConfigs.length,
        科目配置: subjectConfigs.length,
        图片备份: {
          问题图片总数: imageStats.totalQuestionImages,
          解析图片总数: imageStats.totalAnalysisImages,
          成功备份问题图片: imageStats.successfulQuestionImages,
          成功备份解析图片: imageStats.successfulAnalysisImages,
          图片备份成功率: imageStats.totalQuestionImages + imageStats.totalAnalysisImages > 0 
            ? `${((imageStats.successfulQuestionImages + imageStats.successfulAnalysisImages) / (imageStats.totalQuestionImages + imageStats.totalAnalysisImages) * 100).toFixed(1)}%`
            : '无图片'
        }
      });

      if (exportFormat === 'json') {
        // JSON格式导出
        const dataStr = JSON.stringify(exportData, null, 2);
        const dataBlob = new Blob([dataStr], { type: 'application/json' });
        
        const link = document.createElement('a');
        link.href = URL.createObjectURL(dataBlob);
        link.download = `ai-mistake-manager-backup-${new Date().toISOString().split('T')[0]}.json`;
        link.click();
        
        URL.revokeObjectURL(link.href);
      } else {
        // CSV格式导出（仅错题数据）
        const csvHeader = 'ID,科目,创建时间,问题描述,题目类型,标签,OCR文本\n';
        const csvRows = mistakes.map(mistake => {
          const row = [
            mistake.id,
            mistake.subject,
            mistake.created_at,
            `"${mistake.user_question.replace(/"/g, '""')}"`,
            mistake.mistake_type,
            `"${mistake.tags.join(', ')}"`,
            `"${mistake.ocr_text.replace(/"/g, '""')}"`
          ];
          return row.join(',');
        }).join('\n');
        
        const csvContent = csvHeader + csvRows;
        const csvBlob = new Blob([csvContent], { type: 'text/csv;charset=utf-8;' });
        
        const link = document.createElement('a');
        link.href = URL.createObjectURL(csvBlob);
        link.download = `ai-mistake-manager-mistakes-${new Date().toISOString().split('T')[0]}.csv`;
        link.click();
        
        URL.revokeObjectURL(link.href);
      }
      
      alert('数据导出成功！');
    } catch (error) {
      console.error('导出失败:', error);
      alert('导出失败: ' + error);
    } finally {
      setIsExporting(false);
    }
  };

  // 导出系统设置
  const handleExportSettings = async () => {
    setIsExportingSettings(true);
    try {
      console.log('开始导出系统设置...');
      
      // 并行获取系统设置数据
      const [
        apiConfigs,
        modelAssignments,
        subjectConfigs
      ] = await Promise.all([
        TauriAPI.getApiConfigurations(),
        TauriAPI.getModelAssignments(),
        TauriAPI.getAllSubjectConfigs(false) // 包括禁用的配置
      ]);

      // 获取系统设置（尝试获取常见设置键）
      const settingKeys = [
        'theme', 'language', 'auto_save', 'default_subject',
        'api_configs', 'model_assignments', 'model_adapter_options'
      ];
      
      const systemSettings: Record<string, any> = {};
      for (const key of settingKeys) {
        try {
          const value = await TauriAPI.getSetting(key);
          if (value !== null) {
            systemSettings[key] = value;
          }
        } catch (error) {
          console.warn(`获取设置失败 ${key}:`, error);
        }
      }
      
      const exportData = {
        version: '2.0',
        type: 'settings_only',
        timestamp: new Date().toISOString(),
        settings: {
          system_settings: systemSettings,
          api_configurations: apiConfigs,
          model_assignments: modelAssignments,
          subject_configurations: subjectConfigs
        }
      };

      console.log('导出系统设置统计:', {
        系统设置: Object.keys(systemSettings).length,
        API配置: apiConfigs.length,
        科目配置: subjectConfigs.length
      });

      const dataStr = JSON.stringify(exportData, null, 2);
      const dataBlob = new Blob([dataStr], { type: 'application/json' });
      
      const link = document.createElement('a');
      link.href = URL.createObjectURL(dataBlob);
      link.download = `ai-mistake-manager-settings-${new Date().toISOString().split('T')[0]}.json`;
      link.click();
      
      URL.revokeObjectURL(link.href);
      alert('系统设置导出成功！');
    } catch (error) {
      console.error('系统设置导出失败:', error);
      alert('系统设置导出失败: ' + error);
    } finally {
      setIsExportingSettings(false);
    }
  };

  // 导入系统设置
  const handleImportSettings = async () => {
    if (!settingsImportFile) {
      alert('请选择要导入的系统设置文件');
      return;
    }

    setIsImportingSettings(true);
    try {
      const fileContent = await readFileAsText(settingsImportFile);
      const importData = JSON.parse(fileContent);
      
      if (!importData.settings || importData.type !== 'settings_only') {
        throw new Error('无效的系统设置备份文件格式');
      }
      
      // 确认导入
      const settingsInfo = {
        系统设置: Object.keys(importData.settings.system_settings || {}).length,
        API配置: (importData.settings.api_configurations || []).length,
        模型分配: importData.settings.model_assignments ? 1 : 0,
        科目配置: (importData.settings.subject_configurations || []).length
      };

      const confirmMessage = `
即将导入系统设置（备份版本: ${importData.version || '1.0'}）：
- 系统设置: ${settingsInfo.系统设置} 项
- API配置: ${settingsInfo.API配置} 个
- 模型分配: ${settingsInfo.模型分配 > 0 ? '包含' : '无'}
- 科目配置: ${settingsInfo.科目配置} 个
- 备份时间: ${new Date(importData.timestamp).toLocaleString('zh-CN')}

注意：导入将覆盖现有系统设置，是否继续？
      `;
      
      if (!confirm(confirmMessage.trim())) {
        return;
      }
      
      console.log('开始导入系统设置...');
      let importResults = {
        settings: 0,
        apiConfigs: 0,
        modelAssignments: 0,
        subjectConfigs: 0
      };

      // 导入系统设置
      if (importData.settings.system_settings) {
        for (const [key, value] of Object.entries(importData.settings.system_settings)) {
          try {
            await TauriAPI.saveSetting(key, String(value));
            importResults.settings++;
          } catch (error) {
            console.warn(`导入设置失败 ${key}:`, error);
          }
        }
      }

      // 导入API配置
      if (importData.settings.api_configurations) {
        try {
          await TauriAPI.saveApiConfigurations(importData.settings.api_configurations);
          importResults.apiConfigs = importData.settings.api_configurations.length;
          console.log('API配置导入成功');
        } catch (error) {
          console.warn('API配置导入失败:', error);
        }
      }

      // 导入模型分配
      if (importData.settings.model_assignments) {
        try {
          await TauriAPI.saveModelAssignments(importData.settings.model_assignments);
          importResults.modelAssignments = 1;
          console.log('模型分配导入成功');
        } catch (error) {
          console.warn('模型分配导入失败:', error);
        }
      }

      // 导入科目配置
      if (importData.settings.subject_configurations) {
        console.log('科目配置导入功能需要后端API支持，暂时跳过');
      }

      const successMessage = `
系统设置导入完成！统计信息：
- 系统设置：${importResults.settings} 项  
- API配置：${importResults.apiConfigs} 个
- 模型分配：${importResults.modelAssignments > 0 ? '已更新' : '无'}

页面将刷新以应用更改。
      `;
      
      alert(successMessage.trim());
      window.location.reload();
      
    } catch (error) {
      console.error('系统设置导入失败:', error);
      alert('系统设置导入失败: ' + error);
    } finally {
      setIsImportingSettings(false);
      setSettingsImportFile(null);
    }
  };

  // 导入数据
  const handleImport = async () => {
    if (!importFile) {
      alert('请选择要导入的文件');
      return;
    }

    setIsImporting(true);
    try {
      const fileContent = await readFileAsText(importFile);
      
      if (importFile.name.endsWith('.json')) {
        // JSON格式导入
        const importData = JSON.parse(fileContent);
        
        if (!importData.data || !importData.data.mistakes) {
          throw new Error('无效的备份文件格式');
        }
        
        // 确认导入
        const settingsInfo = importData.data.settings ? {
          系统设置: Object.keys(importData.data.settings.system_settings || {}).length,
          API配置: (importData.data.settings.api_configurations || []).length,
          模型分配: importData.data.settings.model_assignments ? 1 : 0,
          科目配置: (importData.data.settings.subject_configurations || []).length
        } : {};

        // 统计图片备份信息
        const imageBackupStats = importData.data.mistakes.reduce((stats: any, mistake: any) => {
          if (mistake.question_image_data) {
            stats.totalQuestionImages += mistake.question_image_data.length;
            stats.backedUpQuestionImages += mistake.question_image_data.filter((img: any) => img.data).length;
          }
          if (mistake.analysis_image_data) {
            stats.totalAnalysisImages += mistake.analysis_image_data.length;
            stats.backedUpAnalysisImages += mistake.analysis_image_data.filter((img: any) => img.data).length;
          }
          return stats;
        }, {
          totalQuestionImages: 0,
          totalAnalysisImages: 0,
          backedUpQuestionImages: 0,
          backedUpAnalysisImages: 0
        });

        const totalImages = imageBackupStats.totalQuestionImages + imageBackupStats.totalAnalysisImages;
        const backedUpImages = imageBackupStats.backedUpQuestionImages + imageBackupStats.backedUpAnalysisImages;

        const confirmMessage = `
即将导入数据（备份版本: ${importData.version || '1.0'}）：
- 错题数量: ${importData.data.mistakes.length}
- 系统设置: ${settingsInfo.系统设置 || 0} 项
- API配置: ${settingsInfo.API配置 || 0} 个
- 模型分配: ${(settingsInfo.模型分配 || 0) > 0 ? '包含' : '无'}
- 科目配置: ${settingsInfo.科目配置 || 0} 个
- 图片备份: ${totalImages > 0 ? `${backedUpImages}/${totalImages} (${(backedUpImages/totalImages*100).toFixed(1)}%)` : '无图片'}
- 备份时间: ${new Date(importData.timestamp).toLocaleString('zh-CN')}

注意：导入将覆盖现有数据，是否继续？
        `;
        
        if (!confirm(confirmMessage.trim())) {
          return;
        }
        
        console.log('开始导入完整数据...');
        let importResults = {
          mistakes: 0,
          settings: 0,
          apiConfigs: 0,
          modelAssignments: 0,
          subjectConfigs: 0
        };

        // 1. 预处理：恢复图片文件
        console.log('开始恢复图片文件...');
        const mistakesToImport = [];
        
        for (const mistake of importData.data.mistakes) {
          try {
            const processedMistake = { ...mistake };
            
            // 🎯 处理问题图片
            if (mistake.question_image_data && mistake.question_image_data.length > 0) {
              const newQuestionImages = [];
              for (const imageInfo of mistake.question_image_data) {
                if (imageInfo.data) {
                  try {
                    // 从备份的Base64数据恢复图片文件
                    const newPath = await TauriAPI.saveImageFromBase64(imageInfo.data, imageInfo.path);
                    newQuestionImages.push(newPath);
                    console.log(`恢复问题图片成功: ${imageInfo.path} -> ${newPath}`);
                  } catch (error) {
                    console.warn(`恢复问题图片失败: ${imageInfo.path}`, error);
                    // 如果恢复失败，使用原路径（可能不存在）
                    newQuestionImages.push(imageInfo.path);
                  }
                } else {
                  // 没有备份数据，使用原路径
                  newQuestionImages.push(imageInfo.path);
                }
              }
              processedMistake.question_images = newQuestionImages;
            }
            
            // 🎯 处理解析图片
            if (mistake.analysis_image_data && mistake.analysis_image_data.length > 0) {
              const newAnalysisImages = [];
              for (const imageInfo of mistake.analysis_image_data) {
                if (imageInfo.data) {
                  try {
                    const newPath = await TauriAPI.saveImageFromBase64(imageInfo.data, imageInfo.path);
                    newAnalysisImages.push(newPath);
                    console.log(`恢复解析图片成功: ${imageInfo.path} -> ${newPath}`);
                  } catch (error) {
                    console.warn(`恢复解析图片失败: ${imageInfo.path}`, error);
                    newAnalysisImages.push(imageInfo.path);
                  }
                } else {
                  newAnalysisImages.push(imageInfo.path);
                }
              }
              processedMistake.analysis_images = newAnalysisImages;
            }
            
            // 清理临时的图片数据字段，避免传递到后端
            delete processedMistake.question_image_data;
            delete processedMistake.analysis_image_data;
            
            mistakesToImport.push(processedMistake);
          } catch (error) {
            console.warn(`处理错题失败: ${mistake.id}`, error);
            mistakesToImport.push(mistake);
          }
        }

        console.log('图片恢复完成，开始导入错题数据...');

        // 2. 导入错题数据 - 使用批量操作
        if (mistakesToImport.length > 0) {
          try {
            await TauriAPI.batchSaveMistakes(mistakesToImport);
            importResults.mistakes = mistakesToImport.length;
            console.log(`批量导入 ${mistakesToImport.length} 道错题成功`);
          } catch (error) {
            console.error('批量导入失败，尝试逐个导入:', error);
            // 如果批量操作失败，回退到逐个保存
            let successCount = 0;
            for (const mistake of mistakesToImport) {
              try {
                await TauriAPI.updateMistake(mistake);
                successCount++;
              } catch (error) {
                console.warn('导入错题失败:', mistake.id, error);
              }
            }
            importResults.mistakes = successCount;
            console.log(`逐个导入完成，成功: ${successCount}/${mistakesToImport.length}`);
          }
        }

        // 2. 导入设置数据
        if (importData.data.settings) {
          // 导入系统设置
          if (importData.data.settings.system_settings) {
            for (const [key, value] of Object.entries(importData.data.settings.system_settings)) {
              try {
                await TauriAPI.saveSetting(key, String(value));
                importResults.settings++;
              } catch (error) {
                console.warn(`导入设置失败 ${key}:`, error);
              }
            }
          }

          // 导入API配置
          if (importData.data.settings.api_configurations) {
            try {
              await TauriAPI.saveApiConfigurations(importData.data.settings.api_configurations);
              importResults.apiConfigs = importData.data.settings.api_configurations.length;
              console.log('API配置导入成功');
            } catch (error) {
              console.warn('API配置导入失败:', error);
            }
          }

          // 导入模型分配
          if (importData.data.settings.model_assignments) {
            try {
              await TauriAPI.saveModelAssignments(importData.data.settings.model_assignments);
              importResults.modelAssignments = 1;
              console.log('模型分配导入成功');
            } catch (error) {
              console.warn('模型分配导入失败:', error);
            }
          }

          // 导入科目配置（如果支持）
          if (importData.data.settings.subject_configurations) {
            // 注意：科目配置导入需要逐个处理，因为可能涉及创建或更新
            for (const config of importData.data.settings.subject_configurations) {
              try {
                // 这里可能需要检查是创建还是更新
                // 为简化，我们尝试更新，如果失败则跳过
                console.log('科目配置导入功能需要后端API支持，暂时跳过');
              } catch (error) {
                console.warn('科目配置导入失败:', error);
              }
            }
          }
        }

        const successMessage = `
导入完成！统计信息：
- 错题：${importResults.mistakes} 条
- 系统设置：${importResults.settings} 项  
- API配置：${importResults.apiConfigs} 个
- 模型分配：${importResults.modelAssignments > 0 ? '已更新' : '无'}

页面将刷新以应用更改。
        `;
        
        alert(successMessage.trim());
        window.location.reload();
        
      } else if (importFile.name.endsWith('.csv')) {
        // CSV格式导入（仅错题数据）
        const lines = fileContent.split('\n');
        const header = lines[0];
        
        if (!header.includes('ID') || !header.includes('科目')) {
          throw new Error('CSV文件格式不正确');
        }
        
        const mistakes = [];
        for (let i = 1; i < lines.length; i++) {
          const line = lines[i].trim();
          if (!line) continue;
          
          const values = parseCSVLine(line);
          if (values.length >= 7) {
            mistakes.push({
              id: values[0],
              subject: values[1],
              created_at: values[2],
              updated_at: values[2], // 使用创建时间作为更新时间
              user_question: values[3],
              mistake_type: values[4],
              tags: values[5].split(', ').filter((tag: string) => tag.trim()),
              ocr_text: values[6],
              question_images: [],
              analysis_images: [],
              status: 'completed',
              chat_history: []
            });
          }
        }
        
        if (mistakes.length === 0) {
          throw new Error('CSV文件中没有有效的错题数据');
        }
        
        if (!confirm(`即将导入 ${mistakes.length} 道错题，是否继续？`)) {
          return;
        }
        
        // 使用批量保存错题
        try {
          await TauriAPI.batchSaveMistakes(mistakes);
          alert(`成功批量导入 ${mistakes.length} 道错题！`);
        } catch (error) {
          console.error('批量导入失败，尝试逐个导入:', error);
          // 如果批量操作失败，回退到逐个保存
          let successCount = 0;
          for (const mistake of mistakes) {
            try {
              await TauriAPI.updateMistake(mistake);
              successCount++;
            } catch (error) {
              console.warn('导入错题失败:', mistake.id, error);
            }
          }
          alert(`成功导入 ${successCount} 道错题（使用逐个导入）！`);
        }
        
      } else {
        throw new Error('不支持的文件格式，请选择 .json 或 .csv 文件');
      }
      
    } catch (error) {
      console.error('导入失败:', error);
      alert('导入失败: ' + error);
    } finally {
      setIsImporting(false);
      setImportFile(null);
    }
  };

  // 清空所有数据 - 三层验证防误触
  const handleClearAllData = async () => {
    // 第一层验证：基本确认
    const confirmMessage = `
⚠️ 危险操作警告 ⚠️

您即将删除所有数据，包括：
- 所有错题记录
- 所有回顾分析
- 所有设置配置

此操作不可恢复！请确认您已经备份了重要数据。

确定要继续吗？
    `;
    
    if (!confirm(confirmMessage.trim())) {
      return;
    }
    
    // 第二层验证：等待3秒冷静期
    const waitMessage = `
正在进入数据清空流程...

请等待 3 秒冷静期，然后会进入最终确认。

如果您改变主意，请关闭此对话框。
    `;
    
    if (!confirm(waitMessage.trim())) {
      return;
    }
    
    // 显示倒计时
    for (let i = 3; i >= 1; i--) {
      await new Promise(resolve => setTimeout(resolve, 1000));
      if (!confirm(`${i} 秒后进入最终确认...\n\n点击"取消"可随时停止操作`)) {
        return;
      }
    }
    
    // 第三层验证：输入确认文本
    const currentTime = new Date().toLocaleTimeString('zh-CN');
    const finalConfirm = prompt(`
🔐 最终安全验证

当前时间: ${currentTime}

请输入以下确认文本来完成数据清空：
"我确认删除所有数据${currentTime.slice(-5)}"

注意：必须完全匹配，包括时间后缀
    `);
    
    const expectedText = `我确认删除所有数据${currentTime.slice(-5)}`;
    if (finalConfirm !== expectedText) {
      alert('确认文本不正确或已超时，操作已取消\n\n为了安全，请重新开始整个验证流程');
      return;
    }
    
    try {
      // 获取所有错题ID并批量删除
      const allMistakes = await TauriAPI.getMistakes();
      const mistakeIds = allMistakes.map(mistake => mistake.id);
      
      if (mistakeIds.length === 0) {
        alert('✅ 数据库已经是空的！');
        return;
      }
      
      try {
        await TauriAPI.batchDeleteMistakes(mistakeIds);
        alert(`✅ 已批量清空 ${mistakeIds.length} 道错题！页面将刷新。`);
      } catch (error) {
        console.error('批量删除失败，尝试逐个删除:', error);
        // 如果批量删除失败，回退到逐个删除
        let deletedCount = 0;
        for (const mistake of allMistakes) {
          try {
            await TauriAPI.deleteMistake(mistake.id);
            deletedCount++;
          } catch (error) {
            console.warn('删除错题失败:', mistake.id, error);
          }
        }
        alert(`✅ 已清空 ${deletedCount} 道错题（使用逐个删除）！页面将刷新。`);
      }
      
      window.location.reload();
    } catch (error) {
      console.error('清空数据失败:', error);
      alert('清空数据失败: ' + error);
    }
  };

  // 读取文件内容
  const readFileAsText = (file: File): Promise<string> => {
    return new Promise((resolve, reject) => {
      const reader = new FileReader();
      reader.onload = (e) => resolve(e.target?.result as string);
      reader.onerror = (e) => reject(e);
      reader.readAsText(file, 'utf-8');
    });
  };

  // 解析CSV行
  const parseCSVLine = (line: string): string[] => {
    const result = [];
    let current = '';
    let inQuotes = false;
    
    for (let i = 0; i < line.length; i++) {
      const char = line[i];
      
      if (char === '"') {
        if (inQuotes && line[i + 1] === '"') {
          current += '"';
          i++; // 跳过下一个引号
        } else {
          inQuotes = !inQuotes;
        }
      } else if (char === ',' && !inQuotes) {
        result.push(current);
        current = '';
      } else {
        current += char;
      }
    }
    
    result.push(current);
    return result;
  };

  return (
    <div style={{
      width: '100%',
      height: '100%',
      overflow: 'auto',
      background: '#f8fafc'
    }}>
      {/* 头部区域 - 统一白色样式 */}
      <div style={{
        background: 'white',
        borderBottom: '1px solid #e5e7eb',
        padding: '24px 32px',
        position: 'relative'
      }}>
        <div style={{
          position: 'absolute',
          top: 0,
          left: 0,
          right: 0,
          height: '4px',
          background: 'linear-gradient(90deg, #667eea, #764ba2)'
        }}></div>
        
        <div style={{ position: 'relative', zIndex: 1 }}>
          <div style={{ display: 'flex', alignItems: 'center', marginBottom: '8px' }}>
            <svg style={{ width: '32px', height: '32px', marginRight: '12px', color: '#667eea' }} fill="none" stroke="currentColor" viewBox="0 0 24 24" strokeWidth={2}>
              <path d="M11 21.73a2 2 0 0 0 2 0l7-4A2 2 0 0 0 21 16V8a2 2 0 0 0-1-1.73l-7-4a2 2 0 0 0-2 0l-7 4A2 2 0 0 0 3 8v8a2 2 0 0 0 1 1.73z" />
              <path d="M12 22V12" />
              <polyline points="3.29 7 12 12 20.71 7" />
              <path d="m7.5 4.27 9 5.15" />
            </svg>
            <h1 style={{ fontSize: '28px', fontWeight: '700', margin: 0, color: '#1f2937' }}>数据管理</h1>
          </div>
          <p style={{ fontSize: '16px', color: '#6b7280', margin: 0, lineHeight: '1.5' }}>
            备份和恢复您的学习数据，确保数据安全和迁移便利
          </p>
        </div>
      </div>

      <div className="data-import-export" style={{ padding: '24px', background: 'transparent' }}>
        {/* 数据导出 */}
        <div className="section" style={{
          backgroundColor: 'white',
          padding: '1.5rem',
          borderRadius: '8px',
          boxShadow: '0 2px 4px rgba(0,0,0,0.1)',
          marginBottom: '1.5rem'
        }}>
          <h4 style={{ display: 'flex', alignItems: 'center', gap: '8px' }}>
            <Upload size={20} />
            数据导出
          </h4>
          <p>将您的错题数据导出为备份文件</p>
          
          <div className="export-options">
            <div className="format-selector">
              <label>导出格式:</label>
              <select 
                value={exportFormat} 
                onChange={(e) => setExportFormat(e.target.value as 'json' | 'csv')}
              >
                <option value="json">JSON (完整备份)</option>
                <option value="csv">CSV (仅错题数据)</option>
              </select>
            </div>
            
            <div className="format-description">
              {exportFormat === 'json' ? (
                <p style={{ display: 'flex', alignItems: 'center', gap: '8px' }}>
                  <Lightbulb size={16} />
                  JSON格式包含完整数据，可用于完整恢复
                </p>
              ) : (
                <p style={{ display: 'flex', alignItems: 'center', gap: '8px' }}>
                  <Lightbulb size={16} />
                  CSV格式仅包含错题基本信息，适合在Excel中查看
                </p>
              )}
            </div>
          </div>
          
          <button 
            onClick={handleExport}
            disabled={isExporting}
            className="export-button"
            style={{ display: 'flex', alignItems: 'center', gap: '8px' }}
          >
            {isExporting ? '导出中...' : (
              <>
                <Download size={16} />
                导出数据
              </>
            )}
          </button>
        </div>

        {/* 数据导入 */}
        <div className="section" style={{
          backgroundColor: 'white',
          padding: '1.5rem',
          borderRadius: '8px',
          boxShadow: '0 2px 4px rgba(0,0,0,0.1)',
          marginBottom: '1.5rem'
        }}>
          <h4 style={{ display: 'flex', alignItems: 'center', gap: '8px' }}>
            <Download size={20} />
            数据导入
          </h4>
          <p>从备份文件恢复您的错题数据</p>
          
          <div className="import-options">
            <div className="file-selector">
              <input
                type="file"
                accept=".json,.csv"
                onChange={(e) => setImportFile(e.target.files?.[0] || null)}
                id="import-file"
              />
              <label htmlFor="import-file" className="file-label">
                {importFile ? importFile.name : '选择备份文件 (.json 或 .csv)'}
              </label>
            </div>
            
            {importFile && (
              <div className="file-info">
                <p style={{ display: 'flex', alignItems: 'center', gap: '8px' }}>
                  <FileText size={16} />
                  文件: {importFile.name}
                </p>
                <p style={{ display: 'flex', alignItems: 'center', gap: '8px' }}>
                  <HardDrive size={16} />
                  大小: {(importFile.size / 1024).toFixed(2)} KB
                </p>
                <p style={{ display: 'flex', alignItems: 'center', gap: '8px' }}>
                  <Calendar size={16} />
                  修改时间: {new Date(importFile.lastModified).toLocaleString('zh-CN')}
                </p>
              </div>
            )}
          </div>
          
          <button 
            onClick={handleImport}
            disabled={isImporting || !importFile}
            className="import-button"
            style={{ display: 'flex', alignItems: 'center', gap: '8px' }}
          >
            {isImporting ? '导入中...' : (
              <>
                <Upload size={16} />
                导入数据
              </>
            )}
          </button>
        </div>

        {/* 系统设置导出/导入 */}
        <div className="section" style={{
          backgroundColor: 'white',
          padding: '1.5rem',
          borderRadius: '8px',
          boxShadow: '0 2px 4px rgba(0,0,0,0.1)',
          marginBottom: '1.5rem'
        }}>
          <h4 style={{ display: 'flex', alignItems: 'center', gap: '8px', marginBottom: '1rem' }}>
            <Settings size={20} />
            系统设置管理
          </h4>
          <p style={{ marginBottom: '1.5rem' }}>单独导出或恢复API、科目、系统配置等设置，不影响错题数据</p>
          
          <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: '1rem' }}>
            {/* 设置导出 */}
            <div style={{
              border: '1px solid #e5e7eb',
              borderRadius: '6px',
              padding: '1rem'
            }}>
              <h5 style={{ margin: '0 0 0.5rem 0', fontSize: '14px', fontWeight: '600' }}>导出系统设置</h5>
              <p style={{ margin: '0 0 1rem 0', fontSize: '13px', color: '#6b7280' }}>
                包含API配置、科目设置、模型分配等
              </p>
              <button 
                onClick={handleExportSettings}
                disabled={isExportingSettings}
                style={{
                  display: 'flex',
                  alignItems: 'center',
                  gap: '6px',
                  padding: '8px 16px',
                  backgroundColor: '#3b82f6',
                  color: 'white',
                  border: 'none',
                  borderRadius: '4px',
                  fontSize: '14px',
                  cursor: isExportingSettings ? 'not-allowed' : 'pointer',
                  opacity: isExportingSettings ? 0.6 : 1
                }}
              >
                {isExportingSettings ? '导出中...' : (
                  <>
                    <Download size={14} />
                    导出设置
                  </>
                )}
              </button>
            </div>

            {/* 设置导入 */}
            <div style={{
              border: '1px solid #e5e7eb',
              borderRadius: '6px',
              padding: '1rem'
            }}>
              <h5 style={{ margin: '0 0 0.5rem 0', fontSize: '14px', fontWeight: '600' }}>恢复系统设置</h5>
              <p style={{ margin: '0 0 1rem 0', fontSize: '13px', color: '#6b7280' }}>
                从设置备份文件恢复配置
              </p>
              
              <div style={{ marginBottom: '0.75rem' }}>
                <input
                  type="file"
                  accept=".json"
                  onChange={(e) => setSettingsImportFile(e.target.files?.[0] || null)}
                  id="settings-import-file"
                  style={{ display: 'none' }}
                />
                <label 
                  htmlFor="settings-import-file"
                  style={{
                    display: 'inline-block',
                    padding: '6px 12px',
                    backgroundColor: '#f3f4f6',
                    border: '1px solid #d1d5db',
                    borderRadius: '4px',
                    fontSize: '13px',
                    cursor: 'pointer',
                    width: '100%',
                    textAlign: 'center',
                    boxSizing: 'border-box'
                  }}
                >
                  {settingsImportFile ? settingsImportFile.name : '选择设置文件'}
                </label>
              </div>
              
              <button 
                onClick={handleImportSettings}
                disabled={isImportingSettings || !settingsImportFile}
                style={{
                  display: 'flex',
                  alignItems: 'center',
                  gap: '6px',
                  padding: '8px 16px',
                  backgroundColor: '#10b981',
                  color: 'white',
                  border: 'none',
                  borderRadius: '4px',
                  fontSize: '14px',
                  cursor: (isImportingSettings || !settingsImportFile) ? 'not-allowed' : 'pointer',
                  opacity: (isImportingSettings || !settingsImportFile) ? 0.6 : 1
                }}
              >
                {isImportingSettings ? '恢复中...' : (
                  <>
                    <Upload size={14} />
                    恢复设置
                  </>
                )}
              </button>
            </div>
          </div>
          
          <div style={{
            marginTop: '1rem',
            padding: '0.75rem',
            backgroundColor: '#f8fafc',
            borderRadius: '4px',
            fontSize: '13px',
            color: '#6b7280'
          }}>
            <div style={{ display: 'flex', alignItems: 'center', gap: '6px', marginBottom: '4px' }}>
              <Lightbulb size={14} />
              <strong>提示：</strong>
            </div>
            <ul style={{ margin: 0, paddingLeft: '1.2rem' }}>
              <li>系统设置备份独立于错题数据，可安全操作</li>
              <li>包含API密钥等敏感信息，请妥善保管备份文件</li>
              <li>恢复设置会覆盖当前配置，操作前请确认</li>
            </ul>
          </div>
        </div>

        {/* 数据统计 */}
        <div className="section" style={{
          backgroundColor: 'white',
          padding: '1.5rem',
          borderRadius: '8px',
          boxShadow: '0 2px 4px rgba(0,0,0,0.1)',
          marginBottom: '1.5rem'
        }}>
          <h4 style={{ display: 'flex', alignItems: 'center', gap: '8px' }}>
            <BarChart3 size={20} />
            当前数据统计
          </h4>
          <DataStats className="data-stats" />
        </div>

        {/* 危险操作 */}
        <div className="section danger-section" style={{
          backgroundColor: 'white',
          padding: '1.5rem',
          borderRadius: '8px',
          boxShadow: '0 2px 4px rgba(0,0,0,0.1)',
          marginBottom: '1.5rem'
        }}>
          <h4 style={{ display: 'flex', alignItems: 'center', gap: '8px' }}>
            <AlertTriangle size={20} color="#dc3545" />
            危险操作
          </h4>
          <p>以下操作将永久删除数据，请谨慎操作</p>
          
          <button 
            onClick={handleClearAllData}
            className="danger-button"
            style={{ display: 'flex', alignItems: 'center', gap: '8px' }}
          >
            <Trash2 size={16} />
            清空所有数据
          </button>
        </div>

        {/* 使用提示 */}
        <div className="section" style={{
          backgroundColor: 'white',
          padding: '1.5rem',
          borderRadius: '8px',
          boxShadow: '0 2px 4px rgba(0,0,0,0.1)',
          marginBottom: '1.5rem'
        }}>
          <h4 style={{ display: 'flex', alignItems: 'center', gap: '8px' }}>
            <Lightbulb size={20} />
            使用提示
          </h4>
          <ul style={{ margin: 0, paddingLeft: '1.2rem' }}>
            <li>建议定期导出数据进行备份</li>
            <li>JSON格式可完整恢复所有数据</li>
            <li>CSV格式适合数据分析和查看</li>
            <li>导入前请确保文件格式正确</li>
          </ul>
        </div>
      </div>
    </div>
  );
}; 