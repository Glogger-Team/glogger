<template>
  <div class="w-56 shrink-0 flex flex-col gap-1">
    <h3 class="text-sm font-medium text-text-secondary mb-1">Characters</h3>
    <div v-if="characters.length === 0" class="text-sm text-text-muted p-2">
      No characters imported yet.
    </div>
    <button
      v-for="char in characters"
      :key="`${char.character_name}-${char.server_name}`"
      class="p-2 text-left bg-transparent border border-border-default rounded cursor-pointer text-sm transition-all hover:bg-surface-elevated hover:border-border-light"
      :class="{ 'bg-surface-elevated! border-accent-gold/40!': selected?.character_name === char.character_name && selected?.server_name === char.server_name }"
      @click="emit('select', char)">
      <div class="font-medium text-text-primary">{{ char.character_name }}</div>
      <div class="text-xs text-text-muted">{{ char.server_name }} · {{ char.snapshot_count }} report{{ char.snapshot_count !== 1 ? 's' : '' }}</div>
    </button>
  </div>
</template>

<script setup lang="ts">
import type { CharacterInfo } from '../../types/database'

defineProps<{
  characters: CharacterInfo[]
  selected: CharacterInfo | null
}>()

const emit = defineEmits<{
  select: [character: CharacterInfo]
}>()
</script>
