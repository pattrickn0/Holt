---
title: Claude's plan usage has two sources, one per view
summary: The terminal view reports usage through the statusline (ask-agent --usage); the chat view (-p) never runs the statusline and reports it as rate_limit_event in the stream.
touches: [crates/core/src/usage.rs, crates/core/src/chat.rs, crates/app/src/deck/chat.rs]
---
Claude Code runs its `statusLine` command only in the interactive TUI. A chat-view tab runs `claude -p` with stream-json,
so `ask-agent --usage` never fires there and the usage bar froze once the tabs defaulted to Chat. The stream sends
`{"type":"rate_limit_event","rate_limit_info":{"unifiedWindows":{"five_hour":{"utilization":0..1,"resetsAt":s},"seven_day":…}}}`
on each turn: `chat::parse_line` turns it into `ChatEvent::Usage` (`usage::from_claude_rate_limits`) and `Deck::on_chat_event`
stores it with `Core::set_usage("claude", …)`, like the bridge's `POST /usage`.

**Why:** the statusline route looked like the only Claude source; nothing failed, the reading just stopped changing.
**How to apply:** a new reading in either view goes through `set_usage` with `updated_ms`; `utilization` is 0..1 (the
statusline's `used_percentage` is 0..100).
