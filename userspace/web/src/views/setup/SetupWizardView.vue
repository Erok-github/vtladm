<script setup lang="ts">
import { ref, onMounted } from 'vue';
import { useRouter } from 'vue-router';
import { NCard, NButton, NInput, NSteps, NStep, NSpace, NSelect } from 'naive-ui';
import { fetchSetupStatus, completeSetup } from '@/api/auth';

const router = useRouter();
const currentStep = ref(1);
const setupRequired = ref(false);

// Step 1
const dbPath = ref('/opt/vtladm/var/vtl.db');
const tapeDir = ref('/opt/vtladm/var/tapes');
const logDir = ref('/opt/vtladm/var/log/vtl');
// Step 2
const vtlKo = ref('/opt/vtladm/kernel/vtl.ko');
const reloadScript = ref('/opt/vtladm/scripts/vtl-kernel-reload.sh');
const personality = ref('vtl');

onMounted(async () => {
  try {
    const s = await fetchSetupStatus();
    if (!s.setup_required) {
      router.replace('/');
      return;
    }
    setupRequired.value = true;
    dbPath.value = s.defaults.db_path;
    tapeDir.value = s.defaults.tape_dir;
    logDir.value = s.defaults.log_dir;
    vtlKo.value = s.defaults.vtl_ko;
    reloadScript.value = s.defaults.kernel_vtl_reload_script;
  } catch {
    router.replace('/login');
  }
});

async function handleComplete() {
  await completeSetup({
    db_path: dbPath.value,
    tape_dir: tapeDir.value,
    log_dir: logDir.value,
    kernel_vtl_reload_script: reloadScript.value,
    vtl_ko: vtlKo.value,
    vtl_reload_scan_delay_ms: 2000,
    run_kernel_reload_now: false,
    personality: personality.value,
  });
  router.replace('/');
}
</script>

<template>
  <div style="max-width: 640px; margin: 40px auto">
    <NCard title="首次设置向导">
      <NSteps :current="currentStep" style="margin-bottom: 24px">
        <NStep title="基础路径" />
        <NStep title="内核模块" />
        <NStep title="完成" />
      </NSteps>

      <!-- Step 1: Paths -->
      <div v-if="currentStep === 1">
        <NSpace vertical>
          <div>
            <span>DB 路径</span>
            <NInput v-model:value="dbPath" />
          </div>
          <div>
            <span>磁带目录</span>
            <NInput v-model:value="tapeDir" />
          </div>
          <div>
            <span>日志目录</span>
            <NInput v-model:value="logDir" />
          </div>
        </NSpace>
        <NSpace justify="end" style="margin-top: 20px">
          <NButton type="primary" @click="currentStep = 2">下一步</NButton>
        </NSpace>
      </div>

      <!-- Step 2: Kernel -->
      <div v-if="currentStep === 2">
        <NSpace vertical>
          <div>
            <span>vtl.ko 路径</span>
            <NInput v-model:value="vtlKo" />
          </div>
          <div>
            <span>重载脚本</span>
            <NInput v-model:value="reloadScript" />
          </div>
          <div>
            <span>INQUIRY 厂商</span>
            <NSelect
              v-model:value="personality"
              :options="[
                { label: 'VTL (默认)', value: 'vtl' },
                { label: 'IBM TS3584', value: 'ibm' },
                { label: 'STK L700', value: 'stk' },
                { label: 'HP MSL6480', value: 'hp' },
              ]"
            />
          </div>
        </NSpace>
        <NSpace justify="space-between" style="margin-top: 20px">
          <NButton @click="currentStep = 1">上一步</NButton>
          <NButton type="primary" @click="handleComplete">完成设置</NButton>
        </NSpace>
      </div>
    </NCard>
  </div>
</template>
