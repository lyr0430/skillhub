# 前端规范

> 前端规范索引页。权威前端架构文档为 [`docs/08-frontend-architecture.md`](../08-frontend-architecture.md)。
> 路由与页面清单来自 `web/src/app/router.tsx` 实测提取，与代码同步。

## 技术栈（已确定）

| 类别 | 选型 |
|------|------|
| 框架 | React 19 + TypeScript 5.7 |
| 构建 | Vite 6 |
| 路由 | TanStack Router（配置式，`web/src/app/router.tsx`） |
| 数据获取 | TanStack Query 5（服务端数据） |
| UI 组件 | shadcn/ui + Radix UI |
| 样式 | Tailwind CSS 3 |
| 本地状态 | Zustand 5（仅纯客户端状态） |
| API 客户端 | openapi-fetch + openapi-typescript |
| 图标 | Lucide React |
| 国际化 | i18next + react-i18next |
| 桌面端 | Tauri 2（`web/src-tauri`） |
| 包管理 | pnpm 10 |
| 测试 | Vitest 4 + Playwright 1.58 |

## 状态管理边界（重要）

- **TanStack Query**：管理所有服务端数据（API 响应缓存、加载/错误状态）。
- **Zustand**：**仅**管理纯客户端状态（UI 偏好、侧边栏展开、主题、当前 namespace 过滤等）。
- **禁止**在 Zustand 中缓存服务端数据。

## 目录结构

```
web/src/
├── app/          # 路由（router.tsx）、布局、Providers
├── features/     # 按业务域切分：admin, auth, governance, namespace,
│                 #   notification, promotion, publish, report, review,
│                 #   search, security-audit, skill, social, suite, token
├── pages/        # 页面级组件
├── api/          # API 客户端；generated/schema.d.ts 由 OpenAPI 生成
├── shared/       # 跨域共享组件/工具
├── i18n/         # 国际化资源
└── types/        # 全局类型
```

API 类型由 `pnpm generate-api` 从 `http://localhost:8080/v3/api-docs` 生成（`src/api/generated/schema.d.ts`）。

## 页面结构（实测路由清单）

### 门户区（公开，匿名可访问）

| 路由 | 说明 |
|------|------|
| `/` | 首页 |
| `/search` | 搜索 |
| `/skills` | 技能列表 |
| `/suites` | 技能套件列表 |
| `/space/$namespace` | 命名空间主页 |
| `/space/$namespace/$slug` | 技能详情 |
| `/space/$namespace/$slug/compare` | 版本对比 |
| `/suite/$namespace/$slug` | 套件详情 |
| `/local-skills` | 本地技能（桌面端托管本地 agent 技能） |
| `/privacy`、`/terms` | 法务页 |

### 认证

`/login`、`/register`、`/reset-password`、`/cli/auth`（CLI 登录回调）

### 个人中心（登录）

`/dashboard`、`/dashboard/skills`、`/dashboard/publish`、`/dashboard/stars`、
`/dashboard/tokens`、`/dashboard/namespaces`、`/dashboard/notifications`、
`/dashboard/subscriptions`、
`/dashboard/suites`、`/dashboard/suites/new`、
`/dashboard/suites/$namespace/$slug/edit`、`/dashboard/suites/$namespace/$slug/new-version`

### 空间管理（空间 ADMIN）

`/dashboard/namespaces/$slug/members`、`/dashboard/namespaces/$slug/reviews`、
`/dashboard/namespaces/$slug/reviews/$id`

### 审核与治理（按角色）

`/dashboard/reviews`、`/dashboard/reviews/$id`、`/dashboard/review-progress`、
`/dashboard/promotions`、`/dashboard/reports`、`/dashboard/governance`

### 平台管理（平台角色）

`/admin/audit-log`、`/admin/labels`、`/admin/namespaces`、`/admin/users`

> 注意：审核与提升的**操作入口在 `dashboard/*`**，`admin/*` 仅保留审计日志、
> 标签、namespace、用户四项平台级管理。二者不可混用。

### 设置

`/settings/profile`、`/settings/security`、`/settings/accounts`、`/settings/notifications`

路由守卫按角色做页面级控制；SUPER_ADMIN 可访问所有管理页。

## 布局

- 门户区：顶部导航 + 内容区，无侧边栏。
- Dashboard / Admin：顶部导航 + 左侧边栏。

## 编码与质量

- 遵循前端设计质量标准（避免模板化 UI、有层级/节奏/深度、意图化配色与字体、设计过的 hover/focus 态）。
- 组件 PascalCase、hooks `use` 前缀、CSS 类 kebab-case、动画仅用 compositor 友好属性（transform/opacity/clip-path）。
- 性能目标：LCP < 2.5s、INP < 200ms、CLS < 0.1；避免渲染阻塞资源、动态引入重型依赖。
- 语义化 HTML 优先；狭窄时单列布局，侧边留白 ≥ 16px。

## 本地开发

```bash
cd web && pnpm install
pnpm dev          # Vite，端口 3000（vite.config.ts server.port）
pnpm test         # vitest run
pnpm test:e2e     # playwright test
pnpm build        # tsc -b && vite build
pnpm lint         # eslint，--max-warnings 0
pnpm typecheck    # tsc --noEmit
pnpm tauri:dev    # Tauri 桌面端
```

## 权威来源

- [`docs/08-frontend-architecture.md`](../08-frontend-architecture.md) — 完整页面/路由/状态边界
- [`docs/18-frontend-annotation-findings.md`](../18-frontend-annotation-findings.md) — 前端注释/标注发现
- `web/LANDING_PAGE_REDESIGN.md`、`web/PREVIEW.md` — 落地页设计说明
