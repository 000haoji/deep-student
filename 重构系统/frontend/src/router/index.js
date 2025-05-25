import { createRouter, createWebHistory } from 'vue-router'

const router = createRouter({
  history: createWebHistory(import.meta.env.BASE_URL),
  routes: [
    {
      path: '/',
      redirect: '/dashboard'
    },
    {
      path: '/',
      component: () => import('@/layouts/MainLayout.vue'),
      children: [
        {
          path: 'dashboard',
          name: 'dashboard',
          component: () => import('@/views/Dashboard.vue')
        },
        {
          path: 'problems',
          name: 'problems',
          component: () => import('@/views/problems/ProblemList.vue')
        },
        {
          path: 'problems/create',
          name: 'problem-create',
          component: () => import('@/views/problems/ProblemCreate.vue')
        },
        {
          path: 'problems/:id',
          name: 'problem-detail',
          component: () => import('@/views/problems/ProblemDetail.vue')
        },
        {
          path: 'problems/:id/edit',
          name: 'problem-edit',
          component: () => import('@/views/problems/ProblemEdit.vue')
        },
        {
          path: 'problems/ai-sessions', // New route for AI session list
          name: 'problem-ai-sessions',
          component: () => import('@/views/problems/AISessionList.vue')
        },
        {
          path: 'analyses',
          name: 'analyses',
          component: () => import('@/views/analyses/AnalysisList.vue')
        },
        {
          path: 'analyses/create',
          name: 'analysis-create',
          component: () => import('@/views/analyses/AnalysisCreate.vue')
        },
        {
          path: 'analyses/:id',
          name: 'analysis-detail',
          component: () => import('@/views/analyses/AnalysisDetail.vue')
        },
        {
          path: 'ai/models',
          name: 'ai-models',
          component: () => import('@/views/ai/AIModelList.vue')
        },
        {
          path: 'ai/stats',
          name: 'ai-stats',
          component: () => import('@/views/ai/AIStats.vue')
        }
      ]
    }
  ]
})

export default router
