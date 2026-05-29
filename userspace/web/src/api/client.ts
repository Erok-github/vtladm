/** CSRF-aware fetch wrapper — all API calls go through here. */

export class ApiError extends Error {
  constructor(
    public status: number,
    message: string,
  ) {
    super(message);
    this.name = 'ApiError';
  }
}

function getCsrfToken(): string {
  const m = document.cookie.match(/(?:^|;\s*)vtl_csrf=([^;]*)/);
  return m ? decodeURIComponent(m[1]) : '';
}

async function request<T>(method: string, path: string, body?: unknown): Promise<T> {
  const headers: Record<string, string> = {};

  if (method !== 'GET') {
    const csrf = getCsrfToken();
    if (csrf) headers['X-VTL-CSRF'] = csrf;
  }
  if (body !== undefined) {
    headers['Content-Type'] = 'application/json';
  }

  const resp = await fetch(path, {
    method,
    headers,
    body: body !== undefined ? JSON.stringify(body) : undefined,
    credentials: 'include',
  });

  // Session expired → redirect to login
  if (resp.status === 401 || resp.status === 403) {
    const data = await resp.json().catch(() => ({}));
    if (resp.status === 401) {
      // 防止在登录页无限重定向
      if (window.location.pathname !== '/login') {
        window.location.href = '/login';
      }
      throw new ApiError(resp.status, 'Unauthorized');
    }
    throw new ApiError(resp.status, data.error || data.message || 'Forbidden');
  }

  const data = await resp.json().catch(() => ({}));

  if (!resp.ok) {
    throw new ApiError(
      resp.status,
      data.error || data.message || `Request failed (${resp.status})`,
    );
  }

  return data as T;
}

export const api = {
  get<T>(path: string): Promise<T> {
    return request<T>('GET', path);
  },
  post<T>(path: string, body?: unknown): Promise<T> {
    return request<T>('POST', path, body);
  },
};
