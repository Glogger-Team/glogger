<template>
  <Teleport to="body">
    <div
      v-if="stallPriceStore.pendingCapture"
      class="fixed bottom-4 right-4 z-50 w-96 max-w-[calc(100vw-2rem)] max-h-[70vh] overflow-y-auto bg-surface-card border border-accent-gold/40 rounded-lg shadow-2xl p-3">
      <!-- Header -->
      <div class="flex items-center justify-between mb-2">
        <h3 class="text-accent-gold text-sm font-semibold m-0">
          Capture stall prices
          <span class="text-status-warning text-[10px] font-semibold uppercase tracking-wide ml-1">experimental</span>
        </h3>
        <button
          class="text-text-muted hover:text-text-primary bg-transparent border-none cursor-pointer text-base leading-none"
          title="Dismiss"
          @click="dismiss">
          &times;
        </button>
      </div>

      <!-- Experimental caveat -->
      <p class="text-text-dim text-[10px] mt-0 mb-2">
        Work in progress, not verified against live stalls. Nothing here overwrites your Market values.
      </p>

      <!-- Stall identity -->
      <div class="flex gap-2 mb-2">
        <div class="flex-1">
          <label class="text-text-muted text-[10px] block mb-0.5">Stall label</label>
          <input
            v-model="stallLabel"
            class="input w-full text-xs"
            placeholder="Unidentified Stall" />
        </div>
        <div class="w-32">
          <label class="text-text-muted text-[10px] block mb-0.5">Owner (optional)</label>
          <input
            v-model="ownerName"
            class="input w-full text-xs"
            placeholder="" />
        </div>
      </div>

      <!-- Quick-entry lines -->
      <label class="text-text-muted text-[10px] block mb-0.5">
        Lines: <code class="text-text-secondary">Item [x&lt;qty>] @ &lt;price></code>
      </label>
      <textarea
        v-model="linesText"
        class="input w-full text-xs font-mono"
        rows="6"
        placeholder="Carrot x80 @ 5&#10;Cantaloupe Wine @ 2500&#10;21890 @ 2500"></textarea>

      <!-- Unresolved lines -->
      <div v-if="unresolved.length > 0" class="mt-1.5 text-red-400 text-[10px]">
        <div class="font-semibold">Unresolved (not saved):</div>
        <div v-for="line in unresolved" :key="line" class="truncate" :title="line">{{ line }}</div>
      </div>

      <!-- OCR debug -->
      <details v-if="ocrDebug" class="mt-1.5">
        <summary class="text-text-dim text-[10px] cursor-pointer select-none">OCR debug (raw)</summary>
        <pre class="text-[10px] text-text-secondary bg-surface-elevated rounded p-1.5 mt-1 whitespace-pre-wrap max-h-32 overflow-y-auto">{{ ocrDebug }}</pre>
      </details>

      <!-- Actions -->
      <div class="flex items-center gap-2 mt-2">
        <button
          class="btn btn-secondary text-xs"
          :disabled="scanning"
          @click="scanWindow">
          {{ scanButtonLabel }}
        </button>
        <button
          class="btn btn-primary text-xs ml-auto"
          :disabled="saveState === 'saving' || parsed.length === 0"
          @click="save">
          {{ saveLabel }}
        </button>
        <button class="btn btn-secondary text-xs" @click="dismiss">Dismiss</button>
      </div>
      <div v-if="saveState === 'error'" class="text-red-400 text-[10px] mt-1">
        {{ saveError }}
      </div>
    </div>
  </Teleport>
</template>

<script setup lang="ts">
import { ref, computed, watch } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { useToast } from '../../composables/useToast'
import { useStallPriceStore } from '../../stores/stallPriceStore'
import { useOcrStore } from '../../stores/ocrStore'
import type { StallPriceEntry, StallScanResult } from '../../types/stallPrices'

const toast = useToast()
const stallPriceStore = useStallPriceStore()
const ocrStore = useOcrStore()

const stallLabel = ref('Unidentified Stall')
const ownerName = ref('')
const linesText = ref('')
const unresolved = ref<string[]>([])
const ocrDebug = ref('')
const scanning = ref(false)
const saveState = ref<'idle' | 'saving' | 'error'>('idle')
const saveError = ref('')

const scanButtonLabel = computed(() => {
  if (scanning.value) return 'Scanning…'
  return ocrStore.installed ? 'Scan window' : 'Install OCR engine…'
})

const saveLabel = computed(() => {
  if (saveState.value === 'saving') return 'Saving…'
  return `Save ${parsed.value.length} prices`
})

/** Parse quick-entry lines: `<item query> [x<qty>] @ <price>`. */
const parsed = computed(() => {
  const out: { entry: StallPriceEntry; raw: string }[] = []
  const unresolvedLines: string[] = []
  for (const rawLine of linesText.value.split('\n')) {
    const line = rawLine.trim()
    if (!line) continue
    const match = line.match(/^(?<item>.+?)(?:\s+x(?<qty>\d+))?\s+@\s*(?<price>\d+[\d,]*)$/)
    if (!match || !match.groups) {
      unresolvedLines.push(line)
      continue
    }
    const item = match.groups.item.trim()
    if (!item) {
      unresolvedLines.push(line)
      continue
    }
    out.push({
      raw: line,
      entry: {
        itemQuery: item,
        quantity: match.groups.qty ? parseInt(match.groups.qty, 10) : 1,
        priceUnit: parseInt(match.groups.price.replace(/,/g, ''), 10),
        stallNpcEntityId: stallPriceStore.pendingCapture?.npcEntityId ?? 0,
        stallLabel: stallLabel.value.trim() || null,
        ownerName: ownerName.value.trim() || null,
        notes: null,
      },
    })
  }
  // Expose unresolved for the template via side effect on a ref.
  unresolved.value = unresolvedLines
  return out
})

// New browse session: reset the form so a different stall's panel is empty.
watch(
  () => stallPriceStore.pendingCapture,
  (pending) => {
    if (pending) {
      linesText.value = ''
      unresolved.value = []
      ocrDebug.value = ''
      stallLabel.value = 'Unidentified Stall'
      ownerName.value = ''
      saveState.value = 'idle'
      saveError.value = ''
      void ocrStore.checkStatus()
    }
  },
)

function dismiss(): void {
  stallPriceStore.dismissCapture()
}

/** OCR scan (Phase B). Installs Tesseract on first use. */
async function scanWindow(): Promise<void> {
  scanning.value = true
  try {
    if (!ocrStore.installed) {
      await ocrStore.download()
    }
    const result = await invoke<StallScanResult>('scan_stall_window')
    ocrDebug.value = result.rows.map((r) => r.raw_text).join('\n')
    // Matched rows become the same quick-entry lines; keep user-typed lines.
    const scanned = result.rows
      .filter((r) => r.item_name && r.price != null)
      .map((r) => `${r.item_name}${r.quantity && r.quantity > 1 ? ` x${r.quantity}` : ''} @ ${r.price}`)
    const existing = new Set(linesText.value.split('\n').map((l) => l.trim()).filter(Boolean))
    const merged = [...linesText.value.split('\n').filter((l) => l.trim()), ...scanned.filter((l) => !existing.has(l.trim()))]
    linesText.value = merged.join('\n')
    if (result.stall_name && stallLabel.value === 'Unidentified Stall') {
      stallLabel.value = result.stall_name
      if (result.owner_name) ownerName.value = result.owner_name
    }
  } catch (e) {
    toast.error('Stall scan failed: ' + String(e))
  } finally {
    scanning.value = false
  }
}

async function save(): Promise<void> {
  if (parsed.value.length === 0) return
  saveState.value = 'saving'
  saveError.value = ''
  try {
    const inserted = await stallPriceStore.recordPrices(parsed.value.map((p) => p.entry))
    toast.success(`Saved ${inserted} prices`)
    linesText.value = ''
    unresolved.value = []
    saveState.value = 'idle'
  } catch (e) {
    saveState.value = 'error'
    saveError.value = String(e)
  }
}
</script>
