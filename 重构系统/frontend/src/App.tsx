import React from 'react';
import { BrowserRouter as Router, Routes, Route } from 'react-router-dom';
import { ConfigProvider } from 'antd';
import zhCN from 'antd/locale/zh_CN';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';

import Layout from './components/Layout';
import Dashboard from './pages/Dashboard';
import ProblemList from './pages/ProblemList';
import ProblemDetail from './pages/ProblemDetail';
import ProblemCreate from './pages/ProblemCreate';
import ReviewAnalysis from './pages/ReviewAnalysis';
import Settings from './pages/Settings';

// 创建 QueryClient 实例
const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      refetchOnWindowFocus: false,
      retry: 1,
    },
  },
});

function App() {
  return (
    <ConfigProvider locale={zhCN}>
      <QueryClientProvider client={queryClient}>
        <Router>
          <Layout>
            <Routes>
              <Route path="/" element={<Dashboard />} />
              <Route path="/problems" element={<ProblemList />} />
              <Route path="/problems/create" element={<ProblemCreate />} />
              <Route path="/problems/:id" element={<ProblemDetail />} />
              <Route path="/analysis" element={<ReviewAnalysis />} />
              <Route path="/settings" element={<Settings />} />
            </Routes>
          </Layout>
        </Router>
      </QueryClientProvider>
    </ConfigProvider>
  );
}

export default App; 