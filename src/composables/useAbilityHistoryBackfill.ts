/**
 * One-shot history import for the cow-curse abilities (pooping / self-milking)
 * plus incoming player milkings (`milked_by`).
 *
 * Poop / self-milk lines only ever appear as `[Combat]` lines, and
 * `"X has milked you."` only ever appears as a `[Status]` line; both channels
 * are excluded from `chat_messages`, so nothing was stored anywhere and
 * position-based chat scans had already run to EOF before the features
 * existed. The backend command sweeps every archived `Chat-*.log` (hundreds
 * of MB — it pre-filters lines, so it is seconds not minutes), so this
 * composable serialises it: the first surface that needs the history triggers
 * a scan, and every other surface (Character → Stats' Dungcrafting group,
 * Dashboard → Milking Timers' self-milked counter) just watches `scanning` /
 * `lastError` and refreshes from the DB afterwards.
 *
 * Callers ask on mount, never "only while my counter reads 0" — a single live
 * event arriving before the first scan would then freeze the rest of the archive
 * out forever.
 */

import { ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'

/** True while the archive scan runs, so callers can show progress. */
export const scanning = ref(false)

/**
 * Why the last scan failed, or `null`. Surfaced by the callers (a silently empty
 * counter is indistinguishable from "you have no archived ability events").
 */
export const lastError = ref<string | null>(null)

/**
 * Attempts made per identity this session. Keyed per character/server, not per
 * session: the command only imports the character it is given, so a session-wide
 * flag would starve every character after the first.
 */
const attempts = new Map<string, number>()

/** Identities whose last attempt failed, i.e. that may use their one retry. */
const failed = new Set<string>()

/** Stop after this many attempts per identity per session. */
const MAX_ATTEMPTS = 2

/** Result of the scan (counts of newly recorded events). */
export interface AbilityHistoryBackfillResult {
  files_scanned: number
  files_skipped: number
  poops_recorded: number
  self_milks_recorded: number
  milked_by_recorded: number
}

/**
 * Import archived ability history for `char`/`server`.
 *
 * At most {@link MAX_ATTEMPTS} attempts per identity per session: the first
 * trigger runs the sweep, and if it *failed* the next trigger retries once, so a
 * transient problem self-heals without repeatedly re-reading the whole archive
 * when the cause is permanent (an unconfigured chat-logs directory is the usual
 * one, and retrying cannot fix it). A successful scan ends it for the session.
 *
 * Never throws: failures are recorded in {@link lastError} and logged.
 */
export async function ensureAbilityHistory(char: string, server: string): Promise<void> {
  const key = `${char}\u0000${server}`
  const made = attempts.get(key) ?? 0
  if (made >= MAX_ATTEMPTS) return
  // One attempt already happened and it worked (or is still in flight).
  if (made >= 1 && !failed.has(key)) return

  attempts.set(key, made + 1)
  failed.delete(key)
  scanning.value = true
  try {
    await invoke<AbilityHistoryBackfillResult>('backfill_ability_history', {
      characterName: char,
      serverName: server,
    })
    lastError.value = null
  } catch (e) {
    failed.add(key)
    lastError.value = String(e)
    console.error('Ability history backfill failed:', e)
  } finally {
    scanning.value = false
  }
}
