import { defineStore } from 'pinia';
import { ref, computed } from 'vue';
import {
  fetchCaptcha,
  login as apiLogin,
  logout as apiLogout,
  changePassword as apiChangePassword,
  fetchSessions,
  revokeSession,
} from '@/api/auth';
import type { SessionInfo } from '@/api/types';

export const useAuthStore = defineStore('auth', () => {
  const sessionValid = ref(false);
  const username = ref('');
  const mustChangePassword = ref(false);
  const captchaId = ref<string | null>(null);
  const captchaQuestion = ref<string | null>(null);
  const loginError = ref<string | null>(null);
  const sessions = ref<SessionInfo[]>([]);

  const isLoggedIn = computed(() => sessionValid.value);

  async function checkSession() {
    try {
      const resp = await fetch('/api/session/ping', { credentials: 'include' });
      if (resp.status === 200) {
        sessionValid.value = true;
        return true;
      }
    } catch {
      // not logged in
    }
    sessionValid.value = false;
    return false;
  }

  async function getCaptcha() {
    try {
      const data = await fetchCaptcha();
      captchaId.value = data.captcha_id;
      captchaQuestion.value = data.question;
      loginError.value = null;
    } catch {
      loginError.value = '无法获取验证码，请检查服务是否正常';
    }
  }

  async function login(password: string, captchaAnswer: string) {
    if (!captchaId.value) return false;
    loginError.value = null;
    try {
      const data = await apiLogin('admin', password, captchaId.value, captchaAnswer);
      if (data.ok) {
        sessionValid.value = true;
        username.value = 'admin';
        mustChangePassword.value = data.must_change_password ?? false;
        return true;
      }
    } catch (e: unknown) {
      loginError.value = e instanceof Error ? e.message : '登录失败';
      await getCaptcha(); // refresh captcha on failure
    }
    return false;
  }

  async function logout() {
    try {
      await apiLogout();
    } catch {
      // ignore
    }
    sessionValid.value = false;
    username.value = '';
    window.location.href = '/login';
  }

  async function changePassword(oldPw: string, newPw: string) {
    return apiChangePassword(oldPw, newPw);
  }

  async function loadSessions() {
    try {
      const data = await fetchSessions();
      sessions.value = data.sessions;
    } catch {
      sessions.value = [];
    }
  }

  async function revoke(token: string) {
    await revokeSession(token);
    await loadSessions();
  }

  return {
    sessionValid,
    username,
    mustChangePassword,
    captchaId,
    captchaQuestion,
    loginError,
    sessions,
    isLoggedIn,
    checkSession,
    getCaptcha,
    login,
    logout,
    changePassword,
    loadSessions,
    revoke,
  };
});
