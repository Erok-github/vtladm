import { createRouter, createWebHistory } from 'vue-router';
import { useAuthStore } from '@/stores/auth';

const router = createRouter({
  history: createWebHistory(),
  routes: [
    {
      path: '/login',
      name: 'login',
      component: () => import('@/views/login/LoginView.vue'),
      meta: { public: true },
    },
    {
      path: '/setup',
      name: 'setup',
      component: () => import('@/views/setup/SetupWizardView.vue'),
      meta: { public: true },
    },
    {
      path: '/',
      component: () => import('@/components/layout/AppLayout.vue'),
      children: [
        {
          path: '',
          name: 'dashboard',
          component: () => import('@/views/dashboard/DashboardView.vue'),
          meta: { title: '仪表盘' },
        },
        {
          path: 'libraries',
          name: 'libraries',
          component: () => import('@/views/libraries/LibraryListView.vue'),
          meta: { title: '磁带库管理' },
        },
        {
          path: 'libraries/:id',
          name: 'library-detail',
          component: () => import('@/views/libraries/LibraryDetailView.vue'),
          meta: { title: '库详情' },
        },
        {
          path: 'tapes',
          name: 'tapes',
          component: () => import('@/views/tapes/TapeListView.vue'),
          meta: { title: '磁带管理' },
        },
        {
          path: 'changer',
          name: 'changer',
          component: () => import('@/views/changer/ChangerReconcileView.vue'),
          meta: { title: '对账面板' },
        },
        {
          path: 'shelves',
          name: 'shelves',
          component: () => import('@/views/shelves/ShelfManageView.vue'),
          meta: { title: '货架管理' },
        },
        {
          path: 'assign-slot',
          name: 'assign-slot',
          component: () => import('@/views/assign/AssignSlotView.vue'),
          meta: { title: '磁带入槽' },
        },
        {
          path: 'transport',
          name: 'transport',
          component: () => import('@/views/transport/TransportView.vue'),
          meta: { title: '传输向导' },
        },
        {
          path: 'account',
          name: 'account',
          component: () => import('@/views/account/AccountView.vue'),
          meta: { title: '账户安全' },
        },
      ],
    },
  ],
});

router.beforeEach(async (to, _from, next) => {
  if (to.meta.public) {
    next();
    return;
  }

  const auth = useAuthStore();
  // Check session if not already validated
  if (!auth.sessionValid) {
    const ok = await auth.checkSession();
    if (!ok) {
      next('/login');
      return;
    }
  }

  if (auth.mustChangePassword && to.name !== 'account') {
    next('/account');
    return;
  }

  next();
});

export default router;
