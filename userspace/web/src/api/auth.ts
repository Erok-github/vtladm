import { api } from './client';
import type { LoginResponse, SessionsResponse, OkResponse, SetupStatusResponse } from './types';

export function fetchSetupStatus() {
  return api.get<SetupStatusResponse>('/api/setup/status');
}

export function completeSetup(body: Record<string, unknown>) {
  return api.post<OkResponse & { kernel_geom: string }>('/api/setup/complete', body);
}

export function fetchCaptcha() {
  return api.get<{ captcha_id: string; question: string }>('/api/captcha');
}

export function login(username: string, password: string, captchaId: string, captchaAnswer: string) {
  return api.post<LoginResponse>('/api/login', {
    username,
    password,
    captcha_id: captchaId,
    captcha_answer: captchaAnswer,
  });
}

export function logout() {
  return api.post<OkResponse>('/api/logout');
}

export function changePassword(oldPassword: string, newPassword: string) {
  return api.post<OkResponse>('/api/change-password', {
    old_password: oldPassword,
    new_password: newPassword,
  });
}

export function fetchSessions() {
  return api.get<SessionsResponse>('/api/sessions');
}

export function revokeSession(token: string) {
  return api.post<OkResponse>('/api/sessions/revoke', { token });
}
