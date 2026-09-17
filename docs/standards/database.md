# 数据库规范

> 数据库规范索引页。权威领域模型文档为 [`docs/02-domain-model.md`](../02-domain-model.md)（表结构、索引、并发约束），部署/迁移见 [`docs/09-deployment.md`](../09-deployment.md)。

## 技术基线

- **PostgreSQL 16.x**，Flyway 管理 schema 迁移。
- **迁移路径：`server/skillhub-app/src/main/resources/db/migration`**（在 app 模块，不在 infra）。
- 当前共 **53 个迁移文件**，最新版本号为 **V53**。新增迁移必须**递增版本号**，禁止修改已应用的历史迁移。
- 迁移文件、SQL 脚本属于**受保护路径**，修改需谨慎，必要时停止并请求确认。

## 迁移与表结构

- 所有 schema 变更通过 Flyway 增量迁移落地，**禁止**手改已生产库。
- 表、索引、约束变更必须说明影响范围（涉及数据、体积、锁）。
- 用户主键、关联字段使用 `string` 类型（见隐性契约），不与整型自增主键混用。
- 时间列统一使用 `TIMESTAMPTZ`（见 [`docs/15-backend-time-governance-plan.md`](../15-backend-time-governance-plan.md)：绝对时间点一律 `TIMESTAMPTZ`）。
  - 已迁：V23（review/idempotency）、V24（api_token）、V25（account_merge_request）、V42（audit_log）等。
  - **迁移仍在进行中**：迁移目录中仍有约 49 处裸 `TIMESTAMP` 未收敛，新增列不得再引入裸 `TIMESTAMP`。

## 查询与索引

- 涉及查询变更时，必须关注**索引、分页、条件范围、N+1 风险**。
- **禁止无条件全表更新/删除**；批量操作必须说明范围控制条件。
- 分页入口由 App Service / query repository 统一处理，避免无限拉取。

### 既有关键索引（已在迁移中核实）

| 表 | 索引名 | 定义 | 用途 |
|----|--------|------|------|
| skill | — | `(namespace_id, status)` | 命名空间内技能列表 |
| skill_version | — | `(skill_id, status)` | 版本列表 |
| review_task | `idx_review_task_namespace_status` | `(namespace_id, status)` | 审核列表 |
| review_task | `idx_review_task_version_pending` | `UNIQUE (skill_version_id) WHERE status='PENDING'` | 单一待审约束 |
| review_task | `idx_review_task_suite_version_pending` | `UNIQUE ... WHERE status='PENDING'`（V50 扩展 typed subject） | 套件版本待审约束 |
| promotion_request | `idx_promotion_request_status` | `(status)` | 待审核列表 |
| promotion_request | `idx_promotion_request_version_pending` | `UNIQUE (source_version_id) WHERE status='PENDING'` | 单一提升申请约束 |
| promotion_request | `idx_promotion_request_target_namespace` | `(target_namespace_id)` | 目标空间查询（V41 补） |
| idempotency_record | `idx_idempotency_record_expires_at` | `(expires_at)` | 过期清理 |

> 全文检索：`skill_search_document.search_vector`（tsvector，自动维护）建 **GIN** 索引 `idx_search_vector`。

## 并发约束（PostgreSQL）

- 审核/提升等"单一 PENDING 记录"约束，统一采用 **partial unique index**（`WHERE status='PENDING'`），见上表 `*_pending` 索引。
- 其余可选方案：`deleted` 软删标记 + 复合唯一索引、或物理删除 + 唯一约束。
- 幂等：Redis SETNX 快速去重 + `idempotency_record` 持久化兜底。
- 乐观锁：版本化实体（如 skill suite version、skill rating）使用 `@Version` 乐观锁字段并发测试覆盖（见 `SkillSuiteVersionOptimisticLockingTest`）。

## 事务与写操作

- 涉及写操作必须**明确事务边界**。
- Controller 不直接调 Mapper/Repository；数据访问封装在 Repository SPI，由 infra 实现。
- 审计字段（owner/creator/updater/reviewer/actor）链路透传，使用字符串主键。

## 验证要求

- 修改 SQL / Mapper 时，**必须至少说明验证方式**（如新增/调整的索引是否被查询命中、EXPLAIN 验证、集成测试覆盖）。
- 新增迁移后，本地需确认 Flyway 能从空库一路 migrate 到最新版本。

## 权威来源

- [`docs/02-domain-model.md`](../02-domain-model.md) — 表结构、索引、并发约束、幂等
- [`docs/09-deployment.md`](../09-deployment.md) — 部署与数据
- [`docs/15-backend-time-governance-plan.md`](../15-backend-time-governance-plan.md) — 时间类型治理（TIMESTAMPTZ 规则与迁移进度）
- [`docs/16-backend-time-inventory.md`](../16-backend-time-inventory.md) — 时间列清单与已完成的迁移
- [`docs/architecture/implicit-contracts.md`](../architecture/implicit-contracts.md) — 并发/幂等/身份约束速查
