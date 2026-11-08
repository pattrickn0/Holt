---
title: Antigravity quota depends on User-Agent and daily-cloudcode-pa host
summary: Only with "antigravity/cli/<recent version>" on daily-cloudcode-pa.googleapis.com does the server return live Gemini/Argon usage.
touches: [crates/core/src/usage/antigravity.rs, crates/core/src/usage.rs]
---
`retrieveUserQuotaSummary` and `fetchAvailableModels` return 200 for any User-Agent, but the response body changes: with
`antigravity/cli/1.4.5 (...)` they return "Gemini Models", "Gemini 4 Argon", and "Claude and GPT models"; with `antigravity-cli`,
`antigravity/cli/1.0.0`, or no version, they return only Claude/GPT, and the model list omits Argon. Token, body, and
project have no effect.

Furthermore, `daily-cloudcode-pa.googleapis.com` must be queried before `cloudcode-pa.googleapis.com`: both return `200 OK`
with the same three groups, but `agy` talks to `daily-cloudcode-pa.googleapis.com` and consumes Gemini/Argon quota there,
while `cloudcode-pa.googleapis.com` leaves Gemini and Argon buckets at 0% used.

**Why:** until 22/10, the app sent `antigravity-cli` and displayed Claude/GPT quota as if it were Gemini's (0%
when Argon was at 94% for the week); later, querying `cloudcode-pa.googleapis.com` first always succeeded with 200 OK and
masked live Gemini/Argon consumption on `daily-cloudcode-pa.googleapis.com`.
**How to apply:** the request mimics the CLI with the version of the installed `agy` (`installed_version`) and queries
`daily-cloudcode-pa.googleapis.com` first (with `cloudcode-pa.googleapis.com` as fallback). If quota appears incorrect,
compare with `agy -p "/quota"`; to see what the CLI sends, point `CLOUD_CODE_URL` to a local server that logs the request.
