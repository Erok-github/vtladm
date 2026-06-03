<script setup lang="ts">
import { ref, onMounted } from 'vue';
import { useRouter, useRoute } from 'vue-router';
import { NForm, NFormItem, NInput, NButton, NCard, NSpace } from 'naive-ui';
import { useAuthStore } from '@/stores/auth';

const auth = useAuthStore();
const router = useRouter();
const route = useRoute();

const password = ref('');
const captchaAnswer = ref('');
const submitting = ref(false);
let loginLock = false;

onMounted(async () => {
  const ok = await auth.checkSession();
  if (ok) {
    router.replace('/');
    return;
  }
  await auth.getCaptcha();
});

async function handleLogin() {
  if (loginLock || !password.value || !captchaAnswer.value) return;
  loginLock = true;
  submitting.value = true;
  try {
    const ok = await auth.login(password.value, captchaAnswer.value);
    if (ok) {
      const redirect = (route.query.redirect as string) || '/';
      router.replace(redirect);
    }
  } finally {
    submitting.value = false;
    loginLock = false;
  }
}
</script>

<template>
  <div class="login-page">
    <NCard class="login-card" :bordered="true">
      <template #header>
        <div class="login-header">
          <span class="login-icon">📼</span>
          <h1>VTL 控制台</h1>
          <p>虚拟磁带库管理系统</p>
        </div>
      </template>

      <NForm>
        <NFormItem label="密码">
          <NInput
            v-model:value="password"
            type="password"
            placeholder="输入密码"
            :disabled="submitting"
            show-password-on="click"
            @keyup.enter="handleLogin"
          />
        </NFormItem>

        <NFormItem v-if="auth.captchaQuestion" label="验证">
          <NSpace align="center">
            <span class="captcha-q">{{ auth.captchaQuestion }}</span>
            <NButton text size="small" type="primary" @click="auth.getCaptcha()">换一题</NButton>
          </NSpace>
          <NInput
            v-model:value="captchaAnswer"
            placeholder="输入答案"
            :disabled="submitting"
            style="margin-top: 8px"
            @keyup.enter="handleLogin"
          />
        </NFormItem>

        <div v-if="auth.loginError" class="login-error">{{ auth.loginError }}</div>

        <NButton
          type="primary"
          block
          :loading="submitting"
          :disabled="submitting"
          @click="handleLogin"
        >
          登 录
        </NButton>
      </NForm>
    </NCard>
    <div class="login-version">v1.0.0</div>
  </div>
</template>

<style scoped>
.login-page {
  min-height: 100vh;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  background: linear-gradient(135deg, #e8ecf4 0%, #d0d9ea 100%);
}

.login-card {
  width: 100%;
  max-width: 400px;
}

.login-header {
  text-align: center;
}

.login-icon {
  font-size: 48px;
}

.login-header h1 {
  margin: 8px 0 4px;
  font-size: 22px;
  color: #0d47a1;
}

.login-header p {
  margin: 0;
  font-size: 14px;
  color: #999;
}

.captcha-q {
  font-family: monospace;
  font-size: 16px;
  font-weight: 600;
  color: #333;
}

.login-error {
  padding: 8px 12px;
  margin-bottom: 12px;
  background: #fff2f0;
  border: 1px solid #ffccc7;
  border-radius: 4px;
  color: #b91c1c;
  font-size: 13px;
}

.login-version {
  margin-top: 16px;
  font-size: 12px;
  color: #bbb;
}
</style>
