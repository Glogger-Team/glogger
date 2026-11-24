import { defineStore, acceptHMRUpdate } from 'pinia'
import { ref, computed } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { useSettingsStore } from './settingsStore'

export interface CharacterResuscitation {
  id: number
  character_name: string
  server_name: string
  occurred_at: string
  caster_name: string
  target_name: string
  success: boolean
  area: string | null
}

export const useResuscitateStore = defineStore('resuscitations', () => {
  const resuscitations = ref<CharacterResuscitation[]>([])
  const loaded = ref(false)

  async function loadResuscitations() {
    if (loaded.value) return
    const settings = useSettingsStore()
    const characterName = settings.settings.activeCharacterName
    const serverName = settings.settings.activeServerName
    if (!characterName || !serverName) return

    try {
      const rows = await invoke<CharacterResuscitation[]>('get_character_resuscitations', {
        characterName,
        serverName,
      })
      // Rapid character switch during the await: a newer load owns the
      // state now — refuse to plant the previous character's rows.
      // (Mirrors the combat/farming baseline guards.)
      const current = useSettingsStore()
      if (
        current.settings.activeCharacterName !== characterName ||
        current.settings.activeServerName !== serverName
      )
        return
      resuscitations.value = rows
      loaded.value = true
    } catch (e) {
      console.error('[resuscitations] Failed to load:', e)
    }
  }

  function resetForCharacter() {
    resuscitations.value = []
    loaded.value = false
  }

  function handleResuscitateEvent(payload: {
    kind: string
    timestamp: string
    caster_name: string
    target_name: string
  }) {
    const settings = useSettingsStore()
    const characterName = settings.settings.activeCharacterName ?? ''
    const serverName = settings.settings.activeServerName ?? ''

    const success = payload.kind === 'Resuscitated'

    resuscitations.value.unshift({
      id: Date.now(),
      character_name: characterName,
      server_name: serverName,
      occurred_at: payload.timestamp,
      caster_name: payload.caster_name,
      target_name: payload.target_name,
      success,
      area: null,
    })
  }

  // --- Computed summaries ---

  /** Successful rezzes only */
  const successfulRezzes = computed(() =>
    resuscitations.value.filter(r => r.success)
  )

  /** Times the active character was rezzed by someone else */
  const rezzedByOthers = computed(() => {
    const settings = useSettingsStore()
    const name = settings.settings.activeCharacterName
    if (!name) return []
    return successfulRezzes.value.filter(r => r.target_name === name)
  })

  /** Times the active character rezzed someone else */
  const rezzedOthers = computed(() => {
    const settings = useSettingsStore()
    const name = settings.settings.activeCharacterName
    if (!name) return []
    return successfulRezzes.value.filter(r => r.caster_name === name)
  })

  /** Top people who rezzed the active character, sorted by count */
  const topRezzers = computed(() => {
    const counts = new Map<string, number>()
    for (const r of rezzedByOthers.value) {
      // Target-only rez lines record caster as "Unknown" — not a person;
      // keep the count in "times rezzed" totals but out of the leaderboard.
      if (r.caster_name === 'Unknown') continue
      counts.set(r.caster_name, (counts.get(r.caster_name) ?? 0) + 1)
    }
    return [...counts.entries()]
      .map(([name, count]) => ({ name, count }))
      .sort((a, b) => b.count - a.count)
  })

  /** Top people the active character has rezzed, sorted by count */
  const topRezzed = computed(() => {
    const counts = new Map<string, number>()
    for (const r of rezzedOthers.value) {
      counts.set(r.target_name, (counts.get(r.target_name) ?? 0) + 1)
    }
    return [...counts.entries()]
      .map(([name, count]) => ({ name, count }))
      .sort((a, b) => b.count - a.count)
  })

  /** Most recent successful rez where someone rezzed the active character.
   *  Skips target-only rows (caster "Unknown") so the widget doesn't show a
   *  placeholder name; falls through to the latest attributed rez. */
  const lastRezzedBy = computed(
    () => rezzedByOthers.value.find(r => r.caster_name !== 'Unknown') ?? null,
  )

  return {
    resuscitations,
    loaded,
    loadResuscitations,
    resetForCharacter,
    handleResuscitateEvent,
    successfulRezzes,
    rezzedByOthers,
    rezzedOthers,
    topRezzers,
    topRezzed,
    lastRezzedBy,
  }
})

if (import.meta.hot) {
  import.meta.hot.accept(acceptHMRUpdate(useResuscitateStore, import.meta.hot))
}
