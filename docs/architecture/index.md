# SkillHub 架构总览

> 本文档是 harness 工作流的架构索引页。**本仓库的权威、完整架构文档是根目录下扁平的 `docs/NN-*.md` 系列**，本索引不复制内容，只做导航与结论摘要，帮助快速定位。

## 产品定位一句话

单实例共享技能注册中心（Skills Hub / Registry），以 ClawHub 为产品蓝本（继承产品模型，不照搬技术实现），以 OpenSkills 借鉴 SKILL.md 格式与目录结构约定。隔离边界是 **namespace**，不是租户。

## 技术基线摘要

| 项 | 选型 |
|----|------|
| JDK | 21 |
| 后端框架 | Spring Boot 3.2.3（多模块单体，Maven） |
| 安全 | Spring Security + OAuth2 Client |
| 数据库 | PostgreSQL 16.x（Flyway 管理，共 53 个迁移，最新 V53） |
| 持久化 | Spring Data JPA（**全仓库无 MyBatis**） |
| 缓存/会话 | Redis 7.x + Redisson（Session + 分布式锁 + 幂等去重） |
| 对象存储 | `LocalFile` + S3 协议兼容双实现 |
| 搜索 | PostgreSQL Full-Text Search（一期，tsvector + GIN） |
| 前端 | React 19 + Vite + TanStack Router/Query + shadcn/Radix + Tailwind |
| 客户端 | Tauri 2 桌面端（`web/src-tauri`）+ Bun CLI（`cli/`） |
| 可观测 | Micrometer + OTLP tracing + Prometheus registry |
| API 文档 | springdoc-openapi（`/v3/api-docs`） |

## 层级与职责

```
Controller(transport) → App Service(workflow 编排) → Domain Service → Repository SPI → infra 实现
```

- **Controller** 只做 transport：鉴权上下文提取、参数绑定、响应包装。不写核心业务逻辑，不直接调 Mapper/Repository。
- **App Service** 负责跨 domain service 协调、分页入口、审计字段传递；复杂 read-model 拼装抽成 **query repository**。
- **domain** 是最内层，只定义接口/实体，不依赖任何其他模块。
- **infra** 实现 domain 定义的 Repository 接口，统一使用 **Spring Data JPA**。
- 禁止 `domain → infra` 方向依赖。

## 后端模块（实测规模）

```
server/
├── skillhub-app          # 364 main / 165 test — 启动 + 装配 + Controller 聚合 + 全局异常/OpenAPI + Flyway 迁移
├── skillhub-domain       # 210 /  48 — 领域模型 + 领域/应用服务
├── skillhub-auth         #  83 /  21 — OAuth2 + RBAC + 授权判定
├── skillhub-infra        #  45 /   3 — JPA、通用工具、配置基础
├── skillhub-search       #  19 /   6 — 搜索 SPI + PostgreSQL 全文实现
├── skillhub-notification #  10 /   4 — 通知
└── skillhub-storage      #   8 /   2 — 对象存储抽象 + LocalFile/S3 双实现
```

模块依赖：`app → domain, auth, search, storage, infra`；`infra/auth/search → domain`；`storage` 为独立 SPI。

> **Flyway 迁移位于 `server/skillhub-app/src/main/resources/db/migration`**（注意不在 infra 模块）。

## 仓库其他模块

```
cli/        # @astron-team/skillhub —— Bun + TypeScript 命令行（agents/clients/commands/services/stores）
scanner/    # Python 技能扫描器（skillhub_scanner_app.py + pytest）
web/src-tauri/  # Tauri 2 桌面端（Rust），一键安装技能到本地 agent
builtin-skills/ # 平台内置技能清单
```

CLI 与桌面端**共用** `/api/cli/v1/**` 兼容层下载技能，不各走一套。

## 权威文档导航

| 关注点 | 权威文档 |
|--------|---------|
| 产品定位与 MVP 范围 | [`docs/00-product-direction.md`](../00-product-direction.md) |
| 系统架构设计 | [`docs/01-system-architecture.md`](../01-system-architecture.md) |
| 领域模型 | [`docs/02-domain-model.md`](../02-domain-model.md) |
| 认证设计 | [`docs/03-authentication-design.md`](../03-authentication-design.md) |
| 搜索架构 | [`docs/04-search-architecture.md`](../04-search-architecture.md) |
| 业务流 | [`docs/05-business-flows.md`](../05-business-flows.md) |
| API 设计 | [`docs/06-api-design.md`](../06-api-design.md) |
| 技能协议 | [`docs/07-skill-protocol.md`](../07-skill-protocol.md) |
| 前端架构 | [`docs/08-frontend-architecture.md`](../08-frontend-architecture.md) |
| 部署 | [`docs/09-deployment.md`](../09-deployment.md) |
| 技能生命周期 | [`docs/14-skill-lifecycle.md`](../14-skill-lifecycle.md) |
| 技能套件 | [`docs/25-skill-suites.md`](../25-skill-suites.md) |
| 可观测性 | [`docs/observability-developer-guide.md`](../observability-developer-guide.md) |
| 隐性业务契约 | [`implicit-contracts.md`](implicit-contracts.md) |

## 变更工件

- 进行中变更：`openspec/changes/add-skill-suites/`（来自上游，非本 fork 产出）
- 已归档变更：见 [`openspec/changes/archive/index.md`](../../openspec/changes/archive/index.md)
- 系统规格：`openspec/specs/`
