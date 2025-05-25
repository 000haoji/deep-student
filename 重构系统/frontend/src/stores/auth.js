import { defineStore } from 'pinia'
import { computed } from 'vue'
// API and router imports might be unnecessary if not used elsewhere in this file.
// If they are truly unused, they can be removed. For now, keeping them commented.
// import api from '@/utils/api'
// import router from '@/router'

export const useAuthStore = defineStore('auth', () => {
  // Since there's no login system, isAuthenticated is always true.
  const isAuthenticated = computed(() => true)

  // All auth-related state (token, user) and functions (login, logout, etc.)
  // have been removed as they are no longer applicable.

  return {
    isAuthenticated
  }
})
