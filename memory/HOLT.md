# Holt: vision and direction

**Holt** is the development environment for AI agents. The product is driven by an AI orchestrator in the user's main tab; the user acts as client and collaborator and approves what goes into `main` (`main` is the stable version; day-to-day work happens on `dev`). How the orchestrator works is in "Working as the orchestrator" below.

## Who it is for

1. **For agents**, first. An agent here must be able to receive a task, work in isolation without stepping on
   others, know what the project has already decided, ask another agent for help, and deliver something reviewable. The explorer,
   Git, GitHub, and memory exist for this.
2. **For the human**, who needs to see what is happening without reading entire terminals: what each agent is
   doing, what is ready, what awaits review, and what went wrong.

## Principles

- **The unit of work is the task, not the message.** Any change to the project (bug fix, feature, validation fix, even a
  small tweak) goes through a task (`ask-agent --task new`), with a brief, an agent with a model, an isolated worktree
  and branch, a report, a diff, and a review; only questions, explanations, investigations, and quick checks that change
  no files are answered in place. Conversation between agents is a means, not the end.
- **Everything visible.** An agent working appears in a tab; no invisible work that the human cannot follow.
- **Isolation by default.** Each task in its own worktree: parallel agents do not interfere with each other, and discarding
  an attempt costs nothing.
- **The right model for each job.** The strongest model available to orchestrate and decide, capable models with free
  quota to develop (`ask-agent --models`), free models (OpenCode) for review and questions, always with findings verified.
  Subscription quota is the scarce resource.
- **Memory lives in files.** What the project decided and learned stays in versioned markdown (this folder), and the
  app delivers the right piece to each agent at the right time.
- **No freezing magic.** No hidden prompts, no frozen dialogs, no orphan processes; errors with explanation.

## What Holt is and is not (user, 2026-10-23)

- **100% Rust**, no other technology (no Electron, Tauri, or web). Systems: **Windows and Linux**; macOS is not on the roadmap.
- **Revolves around agent CLIs** (Claude Code, Antigravity, Codex, OpenCode, and whatever exists), cloud or not:
  whatever is installed is used. **It is not a local model app**: the dedicated Ollama agent (`ollama-agent`) is removed; a local
  model remains possible through a CLI that supports it (e.g. OpenCode).
- **Visible agents**: when an agent needs another, it opens an instance in Holt (tab) via `ask-agent` or the
  tasks desk, **not a CLI-internal subagent**, unless the user explicitly asks.
- **Three memories**: **general** (how to use Holt, identical for everyone, delivered by the app to every CLI in any
  project); **project** (`memory/learnings/` in the user's repository, versioned in it); and **local** (the
  user's and CLIs', which never goes into git).
- **MIT** license. Currently only the user develops; contributions from others, perhaps after release.
- **After `main`:** stability first, then the Windows **installer** ([backlog.md](backlog.md)); themes were set aside (English UI is the default). **Much further ahead**, with the app very stable: monitor and talk to agents **via mobile**.
- Do not do for now: line-by-line comments on diffs to return to the agent ("return with text" is sufficient).
- **Officially supported CLIs** (user, 2026-10-24): **Claude Code, Codex, Antigravity, and OpenCode**. These are tested and
  fully functional (hooks, usage, resume). Others continue opening and working, but without warranty (usage bar
  or hooks may fail) until they enter official support, like Orca, which accepts any CLI.
- **Decided on 2026-10-24** (comparison with MonoCode and Orca; plan revised with `claude@arquiteto`): the X sends
  to the tray and opening again brings the instance back (like Discord); each tab resumes its conversation after
  closing or a crash; task phase comes solely from hooks (never self-declared by agent); durable mail (short
  message typed, long via notice); write scope per task with queue; memory gains a graph as an index (markdown
  files remain the source), measured against what already exists. Items are in [backlog.md](backlog.md).
- Third-party skills: only those that do not conflict with the rules above. From Superpowers 4 were brought in (`.claude/skills/`, with
  `ORIGIN.md`); those that instruct opening subagents, creating worktrees outside the desk, or storing separate plans were
  excluded. OpenSpec was not adopted (would be a second place for the plan).

## Working as the orchestrator

Conventions for whichever agent takes the main tab (on top of [AGENTS.md](../AGENTS.md)):

- **The orchestrator reviews and merges** finished tasks into `dev` itself (`ask-agent --task merge <id>`), resolving conflicts it caused; never hand conflicts or merges back to the user.
- **Decide, do not ask** for what has a sensible default (delegating, model choice, merge order); ask the user only what is theirs to decide (product direction, what goes into `main`).
- **`main` and releases on the user's request:** fast-forward `main` to `dev` after CI passes on `dev`, then a `v*` tag builds and publishes the release ([MAINTENANCE.md](MAINTENANCE.md), section 4).
- **Talking to the user:** the user is Brazilian and practises spoken English through Whisper dictation, and asked to have their English corrected when conversing in English: end English replies with a short "Your English" list (original → natural version, short reason), skipping obvious dictation errors ("hot" for "Holt").
