# Benchmark task (frozen — implement exactly, no scope changes)

## Target

File: `src-tauri/src/chat_status_parser.rs` — the ONLY file you may modify.

## Change

Add a new `ChatStatusEvent` arm for friend presence lines observed in real Chat.log fixtures
(`test_data/surveyLogs/100x-serbcrystal-vanilla/Chat.log` lines 36, 81-82):

- `Your friend Kaeus is now online.`
- `Your friend Sanguinius is now offline.`

1. New enum variant, placed after `MilkedBy` (before the closing brace of the enum):
   `FriendPresence { timestamp: String, friend_name: String, online: bool }`
2. New helper `fn try_friend_presence(text: &str, ts: &str) -> Option<ChatStatusEvent>`
   following the `try_milked_by` / `strip_milked_by_suffix` style:
   - strip prefix `Your friend` + space, then strip suffix `is now online.` (`online = true`)
     or `is now offline.` (`online = false`) — each including its leading space
   - reject empty names; reject names containing `'.'` or `'"'`
     (dotted looks like a sentence fragment, quotes mean chatter — same rationale as milked-by)
3. Insert `.or_else(|| try_friend_presence(text, ts))` as the LAST arm in
   `parse_status_message` (after `try_milked_by`).
4. Tests in `mod tests` using the existing `status_msg` helper, plus `parse_chat_line`
   for the channel-gate test:
   - online line parses with correct `friend_name` and `online == true`
   - offline line parses with `online == false`
   - non-Status channel ignored: a `[Global]` line carrying the same text shape returns `None`
   - near-miss `You have 3 friends online.` returns `None` (existing test must keep passing)
   - empty name (`Your friend  is now online.`) returns `None`
   - dotted name (`Your friend Bob Jr. is now online.`) returns `None`

## Non-goals (do NOT do any of these)

- No changes to `chat_status_dispatch.rs`, `coordinator.rs`, DB, migrations, frontend,
  or any other file. No dispatch arm, no events emitted, no schema changes.

## Acceptance (run all, report outputs)

- `cargo test --lib chat_status_parser` passes (run from `src-tauri/`)
- `cargo clippy --all-targets -- -D warnings` is clean (run from `src-tauri/`)
- Return: the unified diff of your change plus the full test output and clippy output.

Begin by printing RUN-START, finish by printing RUN-END.
