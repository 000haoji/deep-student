import React, { createContext, useContext, useState, useEffect, ReactNode } from 'react';
import { TauriAPI } from '../utils/tauriApi';

interface SubjectConfig {
  id: string;
  subject_name: string;
  display_name: string;
  description: string;
  is_enabled: boolean;
  prompts: any;
  mistake_types: string[];
  default_tags: string[];
  created_at: string;
  updated_at: string;
}

interface SubjectContextType {
  currentSubject: string;
  setCurrentSubject: (subject: string) => void;
  availableSubjects: string[];
  subjectConfigs: SubjectConfig[];
  loading: boolean;
  error: string | null;
  refreshSubjects: () => Promise<void>;
  getEnabledSubjects: () => string[];
  getAllSubjects: () => string[];
}

const SubjectContext = createContext<SubjectContextType | undefined>(undefined);

export const useSubject = () => {
  const context = useContext(SubjectContext);
  if (context === undefined) {
    throw new Error('useSubject must be used within a SubjectProvider');
  }
  return context;
};

interface SubjectProviderProps {
  children: ReactNode;
}

export const SubjectProvider: React.FC<SubjectProviderProps> = ({ children }) => {
  const [currentSubject, setCurrentSubject] = useState<string>('');
  const [availableSubjects, setAvailableSubjects] = useState<string[]>([]);
  const [subjectConfigs, setSubjectConfigs] = useState<SubjectConfig[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  // 包装setCurrentSubject以添加调试日志
  const handleSetCurrentSubject = (newSubject: string) => {
    console.log('🔄 全局科目状态更新:', {
      oldSubject: currentSubject,
      newSubject,
      timestamp: new Date().toISOString()
    });
    setCurrentSubject(newSubject);
  };

  const refreshSubjects = async () => {
    try {
      setLoading(true);
      setError(null);
      
      // 获取所有科目配置
      const configs = await TauriAPI.getAllSubjectConfigs(false);
      setSubjectConfigs(configs);
      
      // 获取支持的科目列表
      const supportedSubjects = await TauriAPI.getSupportedSubjects();
      setAvailableSubjects(supportedSubjects);
      
      // 如果当前没有选择科目，选择第一个启用的科目
      if (!currentSubject && supportedSubjects.length > 0) {
        const enabledConfigs = configs.filter(config => config.is_enabled);
        const firstEnabledSubject = enabledConfigs.length > 0 
          ? enabledConfigs[0].subject_name 
          : supportedSubjects[0];
        handleSetCurrentSubject(firstEnabledSubject);
      }
    } catch (err) {
      console.error('Failed to load subjects:', err);
      setError(err instanceof Error ? err.message : 'Failed to load subjects');
    } finally {
      setLoading(false);
    }
  };

  const getEnabledSubjects = () => {
    return subjectConfigs
      .filter(config => config.is_enabled)
      .map(config => config.subject_name);
  };

  const getAllSubjects = () => {
    return availableSubjects;
  };

  useEffect(() => {
    refreshSubjects();
  }, []);

  const contextValue: SubjectContextType = {
    currentSubject,
    setCurrentSubject: handleSetCurrentSubject,
    availableSubjects,
    subjectConfigs,
    loading,
    error,
    refreshSubjects,
    getEnabledSubjects,
    getAllSubjects,
  };

  return (
    <SubjectContext.Provider value={contextValue}>
      {children}
    </SubjectContext.Provider>
  );
};