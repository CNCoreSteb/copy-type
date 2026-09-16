# AGENTS.md

This file provides guidance to any AI Agent, Coding Assistant or LLM Chat like Codex and Github Copilot when working with code in this repository. Claude Code must use the CLAUDE.md when working with code in this repository.

---

## 核心规则

### 1. 文档创建/修改限制

- **禁止主动创建任何文档**（README、说明文件、注释文档等），除非用户明确要求
- **禁止主动修改现有文档**，除非用户明确要求
- **所有文档必须创建在 `docs/agents` 目录下**，按主题建立子目录以便由用户审查并确定是否采纳，被移出agents目录的文档均代表已被接受，不得在没有用户授权的情况下私自移动，删除，修改。

### 2. 代码修改原则

- 只修改与当前任务直接相关的代码
- 不要进行"顺手"的重构或"改进"
- 不要添加用户未要求的功能
- **如有任何涉及到较大的重构的改进，必须先询问用户的意见后再决定如何处理**

### 3. Git 安全规则

- 永远不要运行 `git push --force`、`git reset --hard`、`--no-verify`
- 只有在用户明确指示时才执行 Git 操作

---

## Conventions

- **Language:** Code, commit messages and commentsin in English.
- **Commit style:** 必须遵循 `提交类型(被修改的模块): 提交描述` 格式
  - 提交类型：`feat` / `fix` / `refactor` / `perf` / `docs` / `chore` / `test` / `style`
  - 被修改的模块：精简表示主要改动范围
  - 提交描述：简明说明改动意图
  - 多行 body 用 HEREDOC 传入保证格式正确
  - **禁止**附加 `Co-Authored-By` 或任何自动生成署名
  - **禁止**不经过用户确认直接提交，每次必须将修改内容的 summary 展示给用户确认后再提交
  - **禁止**提交之后自动Push

---

## 文档索引

所有文档内容统一在 `docs/` 下，按主题查询

| 主题 | 路径 |
| ---- | ---- |
| 暂无 | `暂无` |

---

## 个性化规定

- 每次对话结束时新建一行输出喵字
