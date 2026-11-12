# Holt: overview

Starting point of the documentation, for humans and for AI agents working on the project. It explains
**how Holt works under the hood** (the "environment" where agents run), **what you can do**
with it, and **which rules each document defines**. Technical details are in the other files in this folder.

| Document | Read when |
| :-- | :-- |
| [HOLT.md](HOLT.md) | you want to know where the product is headed, principles, and the order of what comes next |
| [OVERVIEW.md](OVERVIEW.md) (this file) | you want the general map and what can be done in the app |
| [ARCHITECTURE.md](ARCHITECTURE.md) | you are going to work on the bridge, tabs, instances, tasks, or the inter-agent protocol |
| [MAINTENANCE.md](MAINTENANCE.md) | you are going to compile, test, find a file, or follow a recipe (e.g., new agent) |
| [backlog.md](backlog.md) | you want to know the next steps (at most 10, in the order they will be done) |
| [ideas.md](ideas.md) | you want to see undated ideas and known limitations (what is not among the next steps) |
| [history.md](history.md) | you want to know what has already been done (the destination of every completed item from backlog.md) |
| [LINUX.md](LINUX.md) | you are compiling, running, or testing on Linux (Ubuntu and Fedora) and want to know what has been verified and what is pending |

---

## 1. Environment: how Holt works

### The app

Native desktop terminal in Rust (winit + egui). There are two executables, which must remain **in the same folder**:

- `holt` (crate `app`): the window, the terminal tabs, and the right sidebar (tasks desk,
  Git, and GitHub);
- `ask-agent` (crate `cli`): the command agents use to talk to each other and operate tasks.

The "backend" is the `core` crate, with no UI. The UI only talks to it via `Core` methods and receives
`CoreEvent` events back.

### Folders

| Folder | Contents |
| :-- | :-- |
| `workspace/` | the legacy workspace, from the time when there was a global folder. Today each **project** has its own folder (the `cwd` of its tabs); `workspace/` only serves as a migration target from an old config |
| `data/` | runtime data: attachments (`attachments/`), prompts (`prompts/`), hooks (`hooks/`), instructions (`holt-instructions.md`, `agy/<instance>/AGENTS.md`), tasks (`tasks/<project>.json` and `*-brief.md`/`-feedback.md`), sessions, `usage.json`, `crash.log`, `holt.log` (rotates above 2 MB), lock `.holt.lock` and `.holt.bridge` |
| `whisper/` | engine and models for voice dictation (whisper.cpp) |
| `%APPDATA%\Holt\config.json` | configuration. With `HOLT_HOME`, everything stays inside that folder |

**One window per data folder:** opening the app again with the same `data/` brings the live instance back (`POST /activate`)
and exits silently; the window × hides to the system tray (or quits, without a tray) and exit is handled by the icon menu. For another
isolated instance, use `HOLT_HOME`.

### Tabs are instances

Each tab is an **instance** named `model[@role[@name]]`:

- `claude`: the main instance of the model;
- `claude@dev`: an instance with a role;
- `claude@dev@ana`: a second instance of the same model and role, which gets a name.

In addition to the name, each instance has an **LLM model** (`sonnet`, `opus`…), which is not part of the id: it appears next to
the name on the tab and in the sidebar, and is chosen via `ask-agent --model`, the "+ New agent" menu (the `›` arrow for
Claude, Antigravity, and OpenCode lists models; in OpenCode, by provider), or by the `roleModels` default in the
configuration (`{"claude@dev": "sonnet"}`; `model@role` takes precedence over `model`).

The name is the address used by `ask-agent`. Tabs return with the same name and model when the app reopens.

**Environment and instructions:** each tab receives `HOLT_AGENT`, `HOLT_ROLE`, `HOLT_PORT`, `HOLT_TOKEN`, `HOLT_DEPTH`, `HOLT_ASK_WAIT`, and `PATH`; Holt instructions arrive via each CLI's dedicated channel (`AGENTS.md` for CLIs without one) with folder trust pre-registered. Full details and bridge HTTP contract: [ARCHITECTURE.md](ARCHITECTURE.md).

**Inter-agent flow:** `ask-agent <agent> "msg"` opens/routes to the target in the same project, types into its terminal (or saves durable mail in `data/mail/` if >200 chars or multi-line), and returns the `--reply` on stdout (within `HOLT_ASK_WAIT`, default 90 s) or later in the terminal/`--inbox` (HTTP 202). Every task exchange is recorded in the task's **Timeline**.

### The window

| Area | What it has |
| :-- | :-- |
| Title bar | custom (no system title bar): ☰ (settings), collapse sidebar, brand and "Holt", `/ <project>` and branch chip (with the sidebar collapsed, the compact **Agents \| Documents toggle** appears right after); on the right, **usage for each agent with quota** (logo + 5h session and weekly bars; amber starting at 80%, red at 100%; hovering opens the **usage panel** with groups and renewal times) the **notifications bell** (count of unread warnings and errors; opens the history of the last 100 notices, and task notices open their task), and minimize/maximize/close. Notices also show as **toasts** in the bottom-right corner (info and success leave after 6 s, warnings after 10 s, errors stay until closed; hover holds them) |
| Left sidebar | the **Agents \| Documents toggle** at the top, **PROJECTS** (each project with its agents in a tree and model on the right; the active one has "+ New agent"; "+" in the header opens a folder as a project; right-click renames, opens folder, or removes), **FILES**, the explorer (with Git status letter, Filter, Changed only, and Collapse all; Refresh in right-click menu), and **LAYOUT** (`Tabs \| Split \| Highlight`) pinned at the bottom; collapses with `Ctrl+B` |
| Center | two views (`Ctrl+Shift+D`): **Agents** (terminals organized in Tabs, Split, or Highlight; in each pane header are tabs, `⋮⋮` handle, microphone, attachment, and "+") and **Documents** (diff, editable files, and markdown occupying the entire center) |
| Right sidebar | **fixed, does not collapse**; width adjustable via border. Three tabs at the top: **Tasks** (the first; badge counts review + validation), **Git**, and **GitHub** (with open PR count) |

- **Projects:** each project is a folder with its agents. Switching projects hides tabs of the previous one, which
  **keep running**. The tree, Tasks, Git, and GitHub belong to the active project (notifications are global and name the project they came from); usage in the title bar is global.
  Removing and re-adding the same folder reuses the project id and recovers its tasks.
- **Organization (`Tabs | Split | Highlight`):** in **Split**, tabs of the same CLI type share the same pane in an
  automatic grid (2 side-by-side, 2+1, 2×2, 3+2, 3×2), with draggable dividers and a `⋮⋮` handle to rearrange panes;
  in **Highlight**, the pane for the active tab's type occupies 2/3 on the left and the others stack on the right; in **Tabs**,
  all stay in a single pane. The active pane gets a border and halo in the agent's color and the others fade; tab strips that
  do not fit show `‹`/`›` arrows and a `▾` menu. Shortcuts: `Ctrl+Tab` / `Ctrl+Shift+Tab` within the pane, `Ctrl+Alt+arrows` between
  panes, `Ctrl+T` new tab.
- **Documents (`Ctrl+Shift+D`):** opening a diff, file (double click in explorer; single click inserts `@file` into
  terminal), or report brings **Documents** to the center; closing the last one or clicking an agent returns to **Agents**. The "Agents"
  badge counts unseen completed turns (or pulses if an agent is working) and hidden terminals are not
  resized. Text files up to 256 KB have **syntax highlighting** (9 languages), search (`Ctrl+F`), and **editing**
  (`Ctrl+S` preserving CRLF/LF and BOM, with confirmation on close without saving and notification if disk changes externally); up to 1 MB
  open read-only. Diffs feature a file list on the left, highlighting, search, and copy of code only; `.md`, Brief, and
  Report open formatted.
- **Tasks tab:** each task runs in an agent with its own branch and worktree (`.holt/worktrees/tN`), except a **manual task** (created with `--manual`), which has no agent tab: whoever created it works in the worktree and delivers the report. Lifecycle:
  *Running* → (`--task done`) *In review* → (merge) *Waiting for validation* (tab, worktree, and branch remain alive
  to test the app) → (validate) *Completed*; *Reject* returns it to the same agent. The **+ New** form takes title,
  brief, **level** (*Simple*, *Medium*, *Complex*), agent/model (recommended by capability + quota headroom), effort,
  base, **scope** (if it overlaps another active task, it enters **the queue** and starts automatically when cleared), and attachments
  (base and scope fold under "Branch and scope"). The list is a **queue grouped by who moves next**: *Needs you* (review,
  test it, a merge that stopped, a failure; each row carries its two actions and the next one to handle gets the accent
  frame), *Agents working* (working, resolving a conflict with "Cancel automatic merge", queued), *Drafts*, and *Finished*
  (with a filter). Each row has one state glyph, the state word, the agent's logo and model, the id and the diff size; a row
  that needs you carries its main move (Merge, Validate, Ask agent to resolve, Try again) and, at most, the diff. The
  detail opens on a **lifecycle track** (Created → Worked → Review → Merged → Validated, with real times), the explanation
  of the state (queue, files outside the scope, conflict being resolved, merge refusal with its way out), three panes
  (**Report** with the changed files and each file's +/− split under it, **Timeline** with events and agent messages in
  one stream, and **Request**), the actions pinned at the bottom (Return/Reject open their field there) and a "⋯" menu (tab, open in
  Documents, Discard).
- **Git tab and GitHub tab:** the **Git** tab operates the local repository (Changes with stage/discard per file or per
  chunk/hunk, commit, paginated History, Stash, branches with merge/rebase, and Conflict resolution), updating automatically
  when `.git/` changes. The **GitHub** tab lists PRs (with checks), Issues, and Actions (with tail of failed job logs) from the
  `origin` remote, using login credentials from Git Credential Manager or `gh`, and allows **Send to agent**.

### Headless mode

When "Run agents in tabs" is turned off in Settings, `ask-agent` runs agents in **headless mode** (`claude -p`, `agy -p`, `codex exec`, `opencode run`…) with a temporary token (tasks with agents require tabs turned on). Limits and the full HTTP contract live in [ARCHITECTURE.md](ARCHITECTURE.md).

---

## 2. What you can do

**As a user:**

- Open agent tabs from the 20-CLI catalog (Claude Code, Antigravity, Codex, OpenCode, Aider, Goose, Copilot, Gemini,
  Qwen Code, Amazon Q, Open Interpreter, Ollama, Mimo Code, Crush, Cline, Amp, Droid, Continue, llama.cpp, Plandex), add
  any custom CLI via **Add CLI…** (persisted in `config.json` with an automatic non-repeating color and editable/removable
  from the menu), and open regular terminal tabs, with local model names (`Ollama · qwen2.5-coder:7b`, `Mimo Code · mimo-v2.6-9b`)
  shown alongside the agent name.
- Organize work into **projects** (one folder each), view agents from all projects **in a tree** in the sidebar,
  and reorganize the **team** (lead and crew) by dragging one agent onto another.
- View multiple agents at the same time in **split screen** or **highlight** (`Tabs | Split | Highlight`).
- Track **usage** for each agent with quota (session and week) in the title bar and usage panel.
- Talk to an agent via its tab's terminal, reference files (`@file`), attach files/images (button, drag-and-drop,
  or `Ctrl+V`), and use **voice dictation** (`Ctrl+Shift+M` or microphone icon, via local whisper.cpp; ☰ Settings → Dictation sets the spoken language
  or auto-detect, and translating into English).
- Follow task inter-agent messages in the task's **Timeline** and intervene directly in terminals.
- Read and edit project files with syntax highlighting and search, or read diffs and reports full-width in
  **Documents** mode (`Ctrl+Shift+D`).
- Create and manage **tasks** in the Tasks tab (with level, model, effort, scope with automatic queue, diff review,
  assisted merge, and validation step).
- Operate local **Git** (stage per file or hunk, commit, branches, merge/rebase, conflicts, stash) and inspect
  **GitHub** (PRs, issues, Actions, and sending CI failures to an agent).
- Close the window via × to keep agents running in the **system tray**, or exit via the tray menu.

**As an agent, inside a tab:** use `ask-agent --help` for all commands (`--new`, `--model`, `--reply`, `--file`, `--list`, `--team`, `--models`, `--boss`, `--learn`, `--outline`, `--inbox`, `--read`, `--task new|list|show|start|merge|validate|discard|done|return|reject`). Usage rules are in [AGENTS.md](../AGENTS.md) and the HTTP contract is in [ARCHITECTURE.md](ARCHITECTURE.md).

---

## 3. Documents and their rules

Each document covers a specific topic. Changed the topic? Update the document **in the same commit**.

| File | Purpose | Rules it defines / how to maintain |
| :-- | :-- | :-- |
| `memory/OVERVIEW.md` | overview and this map | update when a new document, folder, command, or limit is introduced |
| `memory/HOLT.md` | product vision and direction | the orchestrator decides the order of what comes next; update when direction changes |
| `memory/learnings/` | long-term memory: one file per pitfall, decision, or convention that is not obvious from the code | format and when to create in `learnings/README.md`; `touches` must exist (verified by test `every_learning_in_the_repository_is_valid`) |
| `memory/ARCHITECTURE.md` | stack, crates, bridge protocol, instances, tab delivery, tasks, and memory | **contract**: routes, HTTP codes, environment variables, and bridge limits. Changed the protocol? Update here |
| `memory/MAINTENANCE.md` | practical guide: Rust, commands, code map, recipes | code map and recipes (e.g., new agent: prompt never raw on command line, hooks if CLI supports them) |
| `memory/backlog.md` | next steps, numbered, at most 10 ("Now" and "Later") | completed item leaves `backlog.md` and goes to `history.md` with one line describing what was done; a new item only enters if it fits within the 10 (otherwise goes to `ideas.md`); development only; file does not keep `[x]` |
| `memory/ideas.md` | undated ideas by area, and known limitations | idea that enters `backlog.md` leaves here; new limitation enters here; does not keep completed items |
| `memory/history.md` | what has already been done, newest to oldest (includes the 2026-10-15 audit) | only receives items that left `backlog.md`; one line per item; technical details stay in the commit |
| `memory/LINUX.md` | dependencies, verified items, and manual script for Linux (Ubuntu and Fedora) | update in the same commit that changes a system dependency, font path, data folder, or GPU support; Linux failures become items in `backlog.md` |
| `README.md` (root) | overview, how to build and run, `ask-agent` examples | stays in the root because GitHub displays it on the repository page |
| `CONTRIBUTING.md` (root) | how to contribute and **project principles** | run `cargo fmt`, `clippy -D warnings`, and `cargo test` before PR; privacy; UI never hangs; texts in pt-BR. Stays in the root because GitHub displays it when opening PRs |
| `AGENTS.md` (root) and `workspace/AGENTS.md` | instructions read by agent CLIs | **generated by the app**: block between `<!-- holt:start -->` and `<!-- holt:end -->` is rewritten on each start; edit only outside it. Located in working folder root because CLIs look for them there |
| `workspace/README.md` | what the `workspace/` folder is (legacy workspace and app's `AGENTS.md`) | isolation: agents work in the project or task folder, not in app files |
| `data/README.md` | what goes into `data/` (attachments, prompts, tasks, hooks, `crash.log`) | privacy: audio deleted after transcription, retention of only recent sessions, nothing versioned |
| `whisper/README.md`, `whisper/models/README.md` | engine and models for dictation | located next to the binaries and models they describe |
| `LICENSE` | MIT license | do not edit |

Rule files that are not `.md`: `rustfmt.toml` (code style, checked in CI), `.github/workflows/ci.yml` (what must pass on every push), and `Cargo.toml` (`rust-version` 1.89 is the minimum Rust version).

### General rules, in summary

1. The UI (`app`) only talks to the backend via `Core` methods; the backend responds with `CoreEvent`.
   (ARCHITECTURE, MAINTENANCE)
2. Before finishing any change: `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings`,
   `cargo test --workspace`. (CONTRIBUTING)
3. The bridge only listens on `127.0.0.1`, identifies callers by token, and never trusts `from` in the body.
   Text from another agent never goes raw to a terminal or to the command line. (ARCHITECTURE, CONTRIBUTING)
4. Nothing blocking runs on the UI thread. (CONTRIBUTING)
5. Privacy: audio deleted right after transcription; attachments and prompts kept only for the last 5 sessions;
   never create `AGENTS.md` in the user's home folder. (CONTRIBUTING, data/README)
6. Code comments, docs, and commit messages in English; user-facing text through `t!` with a pt-BR entry; agent-facing
   text in English only. (CONTRIBUTING, learnings/i18n-english-key-and-area-files.md)
7. Documentation alongside code: changed behavior, protocol, or pending work, update the corresponding `.md`
   in `memory/` in the same commit; completed item from `backlog.md` moves to `history.md`. (this document)
