---
name: client-skill-manage-20260911-01
created: 2026-09-11
status: implemented
---

# 需求提案：客户端 Skill 管理功能

## 背景

当前 skillhub 网页端展示技能（skill）时，提供「安装到 agent」操作，但现有交互**只是把一条安装提示词/命令复制到剪贴板**，由用户自行粘贴到目标 agent 中执行。存在以下问题：

- 用户需要手动打开目标 agent（Claude Code、Codex、OpenClaw 等），再粘贴并执行，步骤繁琐、易错。
- 用户不清楚复制的内容是什么、该下载/安装到哪里，缺乏即时反馈。
- 「部署到本地」这一核心价值没有被网页端直接承接，转化路径断裂。
- 缺少对「安装到哪个 agent」「安装到默认全局 skill 位置」等目标的显式选择能力。

## 目标

1. 将现有「安装到 agent」按钮的交互从「复制提示词」升级为「点击后弹出目标选择弹窗」。
2. 弹窗中列出可安装目标：
   - 具体 agent：Claude Code、DeepSeek Harness、Codex、OpenCode、OpenClaw 等。
   - 默认全局 skill 位置（如 `~/.claude/skills`、`~/.agents/skills` 等）。
3. 选择目标后，可将技能**直接安装到本地**（写入目标 agent 的 skill 目录），并给出成功/失败反馈。
4. 探索两种实现路径：
   - **优先：网页端直接实现**——若浏览器安全模型允许（如依赖 File System Access API / 本地桥接服务 / 本地守护进程），直接实现一键安装。
   - **备选：Tauri 封装桌面应用**——当浏览器无法直接写本地文件系统时，用 Tauri 封装一个客户端应用，提供本地安装能力。

## 非目标

- 不实现技能的完整本地生命周期管理（卸载、更新、版本回退）——本期仅聚焦「安装」。
- 不重做现有的发布/审核/治理链路。
- 不处理多平台（Windows/macOS/Linux）的打包与签名分发细节（在 design 阶段确认范围）。
- 不改变技能本身的格式/协议（SKILL.md 约定保持不变）。
- 不实现 agent 的自动发现/探测其内部配置，仅提供在弹窗中手动选择目标的交互。

## 利益相关方

- **技能消费者**：希望在本地直接使用技能的核心用户（最直接受益者）。
- **平台管理员**：依赖技能安装体验提升平台留存。
- **各 agent 生态用户**：Claude Code、DeepSeek Harness、Codex、OpenCode、OpenClaw 用户。
- **前端团队**：实现弹窗与安装交互。
- **（如走 Tauri 路径）桌面端/安全评审团队**：本地文件写入涉及安全边界。

## 验收标准

1. 现有的「安装到 agent」按钮不再只复制提示词；点击后弹出目标选择弹窗。
2. 弹窗展示目标列表，至少包含：Claude Code、DeepSeek Harness、Codex、OpenCode、OpenClaw、默认全局 skill 位置。
3. 选择目标后触发安装，安装成功时给用户明确的成功反馈，失败时给出可读的错误提示。
4. 确定并落地实现路径：要么网页端直接安装（优先），要么 Tauri 桌面应用封装。design.md 中必须明确说明所选路径及理由。
5. 安装结果可通过某种状态（如已安装标记）向用户反馈，避免重复安装造成困惑。
6. 新增到达上述交互的至少一个自动化测试（前端交互测试或端到端测试）。

## 备注

- 用户明确希望「如果网页能直接实现最好」，因此 design 阶段应优先评估浏览器端可行的本地写入路径。
- 涉及本地文件系统写入，需评估安全边界与权限模型（浏览器 File System Access、Tauri fs 权限、本地桥接服务等）。
- 相关既有文档：`docs/07-skill-protocol.md`（技能协议）、`docs/08-frontend-architecture.md`（前端架构）、`docs/14-skill-lifecycle.md`（技能生命周期）。
- 附：目标 agent 示例 —— Claude Code（`~/.claude/skills`）、OpenClaw（`~/.owl/skills` 等）、Codex、OpenCode、DeepSeek Harness。
