<script setup lang="ts">
import { ref, onMounted, h } from 'vue';
import { NCard, NButton, NInput, NDataTable, NSpace, useMessage } from 'naive-ui';
import type { DataTableColumns } from 'naive-ui';
import { useAuthStore } from '@/stores/auth';
import type { SessionInfo } from '@/api/types';

const message = useMessage();
const auth = useAuthStore();

const oldPassword = ref('');
const newPassword = ref('');
const confirmPassword = ref('');

async function handleChangePassword() {
  if (!oldPassword.value || !newPassword.value) {
    message.warning('请填写密码');
    return;
  }
  if (newPassword.value.length < 8) {
    message.warning('新密码至少 8 字符');
    return;
  }
  if (newPassword.value !== confirmPassword.value) {
    message.warning('两次密码不一致');
    return;
  }
  try {
    await auth.changePassword(oldPassword.value, newPassword.value);
    message.success('密码修改成功');
    oldPassword.value = '';
    newPassword.value = '';
    confirmPassword.value = '';
  } catch (e: unknown) {
    message.error(e instanceof Error ? e.message : '修改失败');
  }
}

const sessionCols: DataTableColumns<SessionInfo> = [
  { title: '会话标识', key: 'token_prefix', width: 200 },
  { title: '用户', key: 'username', width: 80 },
  { title: '创建时间', key: 'created_secs_ago', width: 120, render: (r: SessionInfo) => `${r.created_secs_ago} 秒前` },
  {
    title: '操作',
    key: 'actions',
    width: 100,
    render(row) {
      if (row.is_current) return '当前会话';
      return h(NButton, { size: 'tiny', type: 'error', onClick: () => auth.revoke(row.token_prefix) }, { default: () => '撤销' });
    },
  },
];

onMounted(() => {
  auth.loadSessions();
});
</script>

<template>
  <div>
    <h2 style="margin: 0 0 16px; font-size: 20px;">账户与安全</h2>

    <NCard title="修改密码" size="small" style="margin-bottom: 16px; max-width: 480px">
      <NSpace vertical>
        <div>
          <span>当前密码</span>
          <NInput v-model:value="oldPassword" type="password" show-password-on="click" />
        </div>
        <div>
          <span>新密码</span>
          <NInput v-model:value="newPassword" type="password" show-password-on="click" placeholder="最少 8 字符" />
        </div>
        <div>
          <span>确认新密码</span>
          <NInput v-model:value="confirmPassword" type="password" show-password-on="click" />
        </div>
        <NButton type="primary" @click="handleChangePassword">修改密码</NButton>
      </NSpace>
    </NCard>

    <NCard title="活跃会话" size="small" style="max-width: 600px">
      <NDataTable :columns="sessionCols" :data="auth.sessions" :bordered="false" size="small" />
    </NCard>
  </div>
</template>
