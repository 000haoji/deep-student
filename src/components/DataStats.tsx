import { useState, useEffect } from 'react';
import { TauriAPI } from '../utils/tauriApi';

interface DataStatsProps {
  className?: string;
}

export const DataStats: React.FC<DataStatsProps> = ({ className }) => {
  const [mistakeCount, setMistakeCount] = useState(0);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    const loadStats = async () => {
      try {
        const mistakes = await TauriAPI.getMistakes();
        setMistakeCount(mistakes.length);
      } catch (error) {
        console.error('加载统计数据失败:', error);
      } finally {
        setLoading(false);
      }
    };

    loadStats();
  }, []);

  if (loading) {
    return <div className={className}>加载中...</div>;
  }

  return (
    <div className={className}>
      <div className="stat-item">
        <span className="stat-label">错题总数:</span>
        <span className="stat-value">{mistakeCount}</span>
      </div>
      <div className="stat-item">
        <span className="stat-label">回顾分析:</span>
        <span className="stat-value">0</span>
      </div>
      <div className="stat-item">
        <span className="stat-label">存储大小:</span>
        <span className="stat-value">计算中...</span>
      </div>
    </div>
  );
}; 