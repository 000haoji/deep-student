<template>
  <el-container class="main-layout">
    <!-- 侧边栏 -->
    <el-aside :width="isCollapse ? '64px' : '200px'" class="sidebar">
      <div class="logo">
        <h3 v-if="!isCollapse">错题管理系统</h3>
        <h3 v-else>错题</h3>
      </div>
      
      <el-menu
        :default-active="activeMenu"
        :collapse="isCollapse"
        :collapse-transition="false"
        router
        class="sidebar-menu"
      >
        <el-menu-item index="/dashboard">
          <el-icon><DataAnalysis /></el-icon>
          <template #title>数据概览</template>
        </el-menu-item>
        
        <el-sub-menu index="problems-group">
          <template #title>
            <el-icon><Document /></el-icon>
            <span>错题管理</span>
          </template>
          <el-menu-item index="/problems">错题列表</el-menu-item>
          <el-menu-item index="/problems/ai-sessions">AI创建会话</el-menu-item>
        </el-sub-menu>
        
        <el-menu-item index="/analyses">
          <el-icon><TrendCharts /></el-icon>
          <template #title>分析报告</template>
        </el-menu-item>
        
        <el-sub-menu index="ai">
          <template #title>
            <el-icon><Cpu /></el-icon>
            <span>AI管理</span>
          </template>
          <el-menu-item index="/ai/models">模型配置</el-menu-item>
          <el-menu-item index="/ai/stats">使用统计</el-menu-item>
        </el-sub-menu>
      </el-menu>
    </el-aside>

    <el-container>
      <!-- 顶部栏 -->
      <el-header class="header">
        <div class="header-left">
          <el-icon 
            class="collapse-btn" 
            @click="isCollapse = !isCollapse"
          >
            <component :is="isCollapse ? 'Expand' : 'Fold'" />
          </el-icon>
          
          <el-breadcrumb separator="/">
            <el-breadcrumb-item :to="{ path: '/dashboard' }">
              首页
            </el-breadcrumb-item>
            <el-breadcrumb-item v-if="currentRouteName">
              {{ currentRouteName }}
            </el-breadcrumb-item>
          </el-breadcrumb>
        </div>

        <div class="header-right">
          <span class="version-info">本地版本 v2.0</span>
        </div>
      </el-header>

      <!-- 主要内容区域 -->
      <el-main class="main-content">
        <router-view v-slot="{ Component }">
          <transition name="fade" mode="out-in">
            <component :is="Component" />
          </transition>
        </router-view>
      </el-main>
    </el-container>
  </el-container>
</template>

<script setup>
import { ref, computed } from 'vue'
import { useRoute } from 'vue-router'
import { DataAnalysis, Document, TrendCharts, Cpu, Expand, Fold } from '@element-plus/icons-vue'

const route = useRoute()
const isCollapse = ref(false)

const activeMenu = computed(() => route.path)

const currentRouteName = computed(() => {
  const routeNameMap = {
    '/dashboard': '数据概览',
    '/problems': '错题列表', // Updated
    '/problems/create': '创建错题',
    '/problems/ai-sessions': 'AI创建会话', // Added
    '/analyses': '分析报告',
    '/analyses/create': '创建分析',
    '/ai/models': 'AI模型配置',
    '/ai/stats': 'AI使用统计'
  }
  return routeNameMap[route.path] || ''
})
</script>

<style scoped>
.main-layout {
  height: 100vh;
}

.sidebar {
  background-color: #304156;
  transition: width 0.3s;
}

.logo {
  height: 60px;
  display: flex;
  align-items: center;
  justify-content: center;
  color: #fff;
  border-bottom: 1px solid rgba(255, 255, 255, 0.1);
}

.logo h3 {
  margin: 0;
  font-size: 16px;
  white-space: nowrap;
}

.sidebar-menu {
  background-color: #304156;
  border: none;
  height: calc(100% - 60px);
}

.sidebar-menu .el-menu-item {
  color: #bfcbd9;
}

.sidebar-menu .el-menu-item.is-active {
  color: #409eff;
  background-color: #263445;
}

.header {
  background-color: #fff;
  box-shadow: 0 1px 4px rgba(0, 21, 41, 0.08);
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 0 20px;
}

.header-left {
  display: flex;
  align-items: center;
  gap: 20px;
}

.collapse-btn {
  font-size: 20px;
  cursor: pointer;
  transition: color 0.3s;
}

.collapse-btn:hover {
  color: #409eff;
}

.header-right {
  display: flex;
  align-items: center;
  gap: 20px;
}

.version-info {
  color: #909399;
  font-size: 14px;
}

.main-content {
  background-color: #f5f7fa;
  padding: 20px;
}

/* 过渡动画 */
.fade-enter-active,
.fade-leave-active {
  transition: opacity 0.3s;
}

.fade-enter-from,
.fade-leave-to {
  opacity: 0;
}
</style>
