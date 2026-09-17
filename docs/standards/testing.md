# 测试规范

> 测试规范索引页。权威测试文档为 [`docs/e2e.md`](../e2e.md)、[`docs/dev-workflow.md`](../dev-workflow.md)、[`docs/pr-batch-test-runtime.md`](../pr-batch-test-runtime.md)。

## 分层与覆盖目标

| 层 | 工具 | 位置 | 关注点 |
|----|------|------|--------|
| 后端单元测试 | JUnit 5 | `server/*/src/test` | Service/领域逻辑、纯函数 |
| 后端集成测试 | Spring Boot Test + **Testcontainers** | `server/skillhub-app/src/test/.../integration`、`.../repository` | Repository/API 行为、乐观锁并发 |
| 前端单元测试 | Vitest 4 | `web/src`（与源码同目录 `*.test.tsx`） | 组件、hooks、工具函数 |
| 前端 E2E | Playwright 1.58（真实请求模式） | `web/e2e` | 关键用户流、路由守卫 |
| CLI 单元测试 | Bun test | `cli/test` | 命令、客户端、store 逻辑 |
| 扫描器测试 | pytest | `scanner/tests` | 扫描规则 |

实测规模：后端 249 个测试类（app 165 / domain 48 / auth 21 / search 6 / notification 4 / infra 3 / storage 2）。
Testcontainers 已真实使用（如 `SkillSuiteVersionOptimisticLockingTest`、`JpaReviewProgressQueryRepositoryTest`）。

> **覆盖目标**：核心逻辑覆盖 ≥ 80%，可按模块/分支设定。行为变更必须补测试。

## 行为变更必测

- **任何行为变化**都必须补充至少一个相关测试。
- **Service 层变更**：优先补单元测试。
- **Controller/API 行为变更**：优先补集成测试。
- **Bug 修复**：条件允许时补 regression test。
- **SQL / Mapper 修改**：必须至少说明验证方式。

## 测试驱动（TDD）

1. 先写测试（RED）→ 运行失败
2. 写最小实现（GREEN）→ 运行通过
3. 重构（IMPROVE）
4. 验证覆盖 ≥ 80%

## 前端测试约定

- 单元测试用 Vitest；E2E 用 Playwright。
- **避免 flaky 的 timeout 断言**，优先确定性等待。
- E2E 采用**真实请求（non-mock）**模式，通过 `page.context().request` 与后端真实认证/数据交互，不再使用 `page.route` 拦截 API。会话 helper：`web/e2e/helpers/session.ts`。
- Playwright 当前配置（`web/playwright.config.ts`）：`workers: 1`、`fullyParallel: false`、`trace: 'on-first-retry'`、`screenshot: 'on'`。
- 至少覆盖断点：320 / 768 / 1024 / 1440。

## 本地运行（Makefile 为准）

```bash
make dev-all                  # 启动本地全栈（依赖 + scanner + 后端 + 前端）
make test                     # = test-backend + test-frontend
make test-backend             # 后端完整单元测试
make test-backend-app         # 仅 skillhub-app 及其依赖模块
make test-frontend            # 前端 vitest（自动确保 web-deps）
make test-e2e-frontend        # 前端 E2E（Playwright）
make test-e2e-smoke-frontend  # 前端 E2E smoke
make test-cli                 # CLI 单元测试
make test-builtin-skills      # 校验内置 Skills 清单、打包结果与安全边界
make check                    # = build + test（提交前完整校验）

# 也可直接进入子目录
cd web && pnpm test           # vitest run
cd web && pnpm test:e2e       # playwright test
```

> 注意 Makefile 的 E2E target 名为 `test-e2e-frontend`，**没有** `make test-e2e`。

## 验证即完成

- 声称"完成/已修复/测试通过"前，必须运行验证命令并确认输出。
- 始终用证据（测试输出、覆盖率报告）支撑断言。

## 权威来源

- [`docs/e2e.md`](../e2e.md) — Web E2E 全量说明
- [`docs/dev-workflow.md`](../dev-workflow.md) — 本地开发与 mock 认证
- [`docs/pr-batch-test-runtime.md`](../pr-batch-test-runtime.md) — PR 批测运行时
