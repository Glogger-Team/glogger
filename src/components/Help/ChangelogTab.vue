<template>
  <div class="flex flex-col gap-3">
    <!-- Update banner (when update available) -->
    <div
      v-if="updateStore.updateAvailable"
      class="bg-accent-blue/10 border border-accent-blue/30 rounded-lg p-4 flex items-center justify-between gap-4">
      <div>
        <div class="dialog-title">
          Glogger v{{ updateStore.latestVersion }} is available
        </div>
        <div class="text-xs text-text-secondary mt-0.5">
          Review the changes below, then update when you're ready.
        </div>
      </div>
      <div class="flex items-center gap-2 shrink-0">
        <template v-if="updateStore.installing">
          <span class="text-xs text-accent-blue">Updating... {{ updateStore.downloadProgress }}%</span>
        </template>
        <template v-else>
          <button
            class="px-4 py-2 text-sm font-medium rounded cursor-pointer bg-accent-blue/20 border border-accent-blue/40 text-accent-blue hover:bg-accent-blue/30 transition-colors"
            @click="updateStore.downloadAndInstall()">
            Update Glogger
          </button>
        </template>
      </div>
      <div v-if="updateStore.installError" class="text-xs text-accent-red mt-1 w-full">
        {{ updateStore.installError }}
      </div>
    </div>

    <!-- Check for updates (when no update detected yet) -->
    <div v-else class="flex items-center gap-3">
      <button
        class="px-3 py-1.5 text-xs font-medium rounded cursor-pointer bg-surface-base border border-border-default text-text-secondary hover:bg-surface-elevated hover:text-text-primary transition-colors"
        :disabled="checking"
        @click="manualCheck">
        {{ checking ? 'Checking...' : 'Check for Updates' }}
      </button>
      <span v-if="checkResult" class="text-xs text-accent-green">{{ checkResult }}</span>
    </div>

    <!-- Loading -->
    <div v-if="loading" class="text-sm text-text-muted py-8 text-center">
      Loading release notes...
    </div>

    <!-- Error -->
    <div v-else-if="error" class="text-sm text-text-muted py-8 text-center">
      <p>{{ error }}</p>
      <button class="btn btn-primary mt-3" @click="loadReleases">Retry</button>
    </div>

    <!-- Releases -->
    <template v-else>
      <AccordionSection
        v-for="(release, index) in releases"
        :key="release.tag_name"
        :default-open="index === 0">
        <template #title>
          <span class="flex items-center gap-2">
            <span class="text-accent-gold font-bold text-sm">{{ release.tag_name }}</span>
            <span v-if="release.name && release.name !== release.tag_name" class="text-text-muted text-xs">
              {{ release.name }}
            </span>
          </span>
        </template>
        <template #badge>
          <span class="text-text-dim text-xs font-normal">{{ formatDate(release.published_at) }}</span>
        </template>

        <div class="flex flex-col gap-3 pt-1">
          <template v-for="(section, sIdx) in parseReleaseBody(release.body)" :key="sIdx">
            <!-- Section with a heading — auto-collapse downloads -->
            <details v-if="section.heading && isCollapsedHeading(section.heading)" class="group">
              <summary class="flex items-center gap-2 cursor-pointer list-none mb-1.5">
                <span class="text-text-muted text-xs w-4 group-open:rotate-90 transition-transform">&#x25B6;</span>
                <span
                  class="inline-flex items-center px-1.5 py-0.5 rounded text-[10px] font-bold uppercase tracking-wider"
                  :class="categoryBadgeClass(section.heading)">
                  {{ categoryLabel(section.heading) }}
                </span>
                <span class="text-text-dim text-xs">{{ section.items.length }} items</span>
              </summary>
              <ul class="flex flex-col gap-0.5 pl-3 mt-1">
                <li
                  v-for="(item, iIdx) in section.items"
                  :key="iIdx"
                  class="text-sm text-text-secondary leading-relaxed list-disc ml-1 marker:text-text-dim"
                  v-html="renderInlineMarkdown(item)" />
              </ul>
            </details>

            <!-- Section with a heading (categorized changes) -->
            <div v-else-if="section.heading">
              <div class="flex items-center gap-2 mb-1.5">
                <span
                  class="inline-flex items-center px-1.5 py-0.5 rounded text-[10px] font-bold uppercase tracking-wider"
                  :class="categoryBadgeClass(section.heading)">
                  {{ categoryLabel(section.heading) }}
                </span>
              </div>
              <ul class="flex flex-col gap-0.5 pl-3">
                <li
                  v-for="(item, iIdx) in section.items"
                  :key="iIdx"
                  class="text-sm text-text-secondary leading-relaxed list-disc ml-1 marker:text-text-dim"
                  v-html="renderInlineMarkdown(item)" />
              </ul>
            </div>

            <!-- Uncategorized items (no heading) -->
            <div v-else-if="section.items.length > 0">
              <ul class="flex flex-col gap-0.5 pl-3">
                <li
                  v-for="(item, iIdx) in section.items"
                  :key="iIdx"
                  class="text-sm text-text-secondary leading-relaxed list-disc ml-1 marker:text-text-dim"
                  v-html="renderInlineMarkdown(item)" />
              </ul>
            </div>

            <!-- Prose paragraphs (not list items) -->
            <p
              v-else-if="section.prose"
              class="text-sm text-text-secondary leading-relaxed m-0"
              v-html="renderInlineMarkdown(section.prose)" />
          </template>

          <!-- Fallback: if parsing yields nothing, show raw body -->
          <div
            v-if="parseReleaseBody(release.body).length === 0 && release.body"
            class="text-sm text-text-secondary leading-relaxed whitespace-pre-line">
            {{ release.body }}
          </div>
        </div>
      </AccordionSection>

      <div v-if="releases.length === 0" class="text-sm text-text-muted py-8 text-center">
        No releases found.
      </div>
    </template>
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { useUpdateStore } from '../../stores/updateStore'
import AccordionSection from '../Shared/AccordionSection.vue'

const updateStore = useUpdateStore()

const checking = ref(false)
const checkResult = ref<string | null>(null)

async function manualCheck() {
  checking.value = true
  checkResult.value = null
  await updateStore.checkForUpdate(false)
  checking.value = false
  if (updateStore.updateAvailable) {
    checkResult.value = null
  } else {
    checkResult.value = 'You\'re on the latest version.'
  }
}

interface ReleaseInfo {
  tag_name: string
  name: string
  published_at: string
  html_url: string
  body: string
}

interface ReleaseSection {
  heading: string | null
  items: string[]
  prose: string | null
}

const releases = ref<ReleaseInfo[]>([])
const loading = ref(true)
const error = ref('')

async function loadReleases() {
  loading.value = true
  error.value = ''
  try {
    releases.value = await invoke<ReleaseInfo[]>('fetch_github_releases')
  } catch (e) {
    error.value = `Could not load changelog: ${e}`
  } finally {
    loading.value = false
  }
}

function formatDate(iso: string): string {
  if (!iso) return ''
  const d = new Date(iso)
  return d.toLocaleDateString('en-US', { year: 'numeric', month: 'short', day: 'numeric' })
}

/**
 * Parse a GitHub release body into structured sections.
 * Handles ## and ### headings as category separators, list items, and prose text.
 */
/**
 * Headings whose content should be auto-collapsed in the changelog.
 * Users viewing in-app don't need download links prominently displayed.
 */
const COLLAPSED_HEADINGS = new Set(['downloads', 'download', 'installers', 'assets'])

function isCollapsedHeading(heading: string | null): boolean {
  if (!heading) return false
  return COLLAPSED_HEADINGS.has(heading.toLowerCase().trim())
}

function parseReleaseBody(body: string): ReleaseSection[] {
  if (!body) return []

  const lines = body.split('\n')
  const sections: ReleaseSection[] = []
  let currentHeading: string | null = null
  let currentItems: string[] = []
  let currentProse: string[] = []
  let inTable = false

  function flushSection() {
    if (currentProse.length > 0) {
      const proseText = currentProse.join('\n').trim()
      if (proseText) {
        sections.push({ heading: null, items: [], prose: proseText })
      }
      currentProse = []
    }
    if (currentHeading !== null || currentItems.length > 0) {
      if (currentItems.length > 0 || currentHeading) {
        sections.push({ heading: currentHeading, items: [...currentItems], prose: null })
      }
      currentHeading = null
      currentItems = []
    }
  }

  for (const line of lines) {
    const trimmed = line.trim()

    // Heading (## or ###)
    const headingMatch = trimmed.match(/^#{2,3}\s+(.+)$/)
    if (headingMatch) {
      flushSection()
      inTable = false
      currentHeading = headingMatch[1].trim()
      continue
    }

    // Table rows (lines starting and ending with |) — convert to list items
    if (trimmed.startsWith('|') && trimmed.endsWith('|')) {
      // Skip separator rows (|---|---|)
      if (/^\|[\s\-:|]+\|$/.test(trimmed)) {
        inTable = true
        continue
      }
      // Skip header rows (first row before separator)
      if (!inTable) {
        inTable = true
        continue
      }
      // Parse table data row: extract cell values
      const cells = trimmed.split('|').filter(c => c.trim()).map(c => c.trim())
      if (cells.length > 0) {
        currentItems.push(cells.join(' — '))
      }
      continue
    }

    // Non-table line resets table state
    if (inTable && trimmed) inTable = false

    // List item (- or *)
    const listMatch = trimmed.match(/^[-*]\s+(.+)$/)
    if (listMatch) {
      // Flush any accumulated prose before starting list items
      if (currentProse.length > 0) {
        const proseText = currentProse.join('\n').trim()
        if (proseText) {
          sections.push({ heading: null, items: [], prose: proseText })
        }
        currentProse = []
      }
      currentItems.push(listMatch[1])
      continue
    }

    // Empty line
    if (!trimmed) {
      // If we have items accumulated, an empty line doesn't break the section
      // But if we only have prose, flush it
      if (currentItems.length === 0 && currentHeading === null && currentProse.length > 0) {
        flushSection()
      }
      continue
    }

    // Horizontal rule (--- or ***)
    if (/^[-*_]{3,}$/.test(trimmed)) {
      flushSection()
      continue
    }

    // Regular text (prose)
    if (currentItems.length > 0 || currentHeading !== null) {
      // If we were collecting list items under a heading, flush and start prose
      flushSection()
    }
    currentProse.push(trimmed)
  }

  flushSection()
  return sections
}

/**
 * Render inline markdown (bold, inline code, links) to HTML.
 */
function renderInlineMarkdown(text: string): string {
  return text
    // Links [text](url)
    .replace(/\[([^\]]+)\]\(([^)]+)\)/g, '<a href="$2" target="_blank" class="text-accent-blue hover:underline">$1</a>')
    // Bold
    .replace(/\*\*(.+?)\*\*/g, '<strong class="text-text-primary font-semibold">$1</strong>')
    // Inline code
    .replace(/`([^`]+)`/g, '<code class="text-accent-gold bg-surface-dark px-1 py-0.5 rounded-sm text-xs font-mono">$1</code>')
}

/**
 * Map heading text to a badge style class.
 */
function categoryBadgeClass(heading: string): string {
  const h = heading.toLowerCase()
  if (h.includes('feature') || h.includes('new') || h.includes('added')) {
    return 'bg-accent-green/15 text-accent-green border border-accent-green/30'
  }
  if (h.includes('fix') || h.includes('bug')) {
    return 'bg-accent-red/15 text-accent-red border border-accent-red/30'
  }
  if (h.includes('improve') || h.includes('enhance') || h.includes('change') || h.includes('update')) {
    return 'bg-accent-blue/15 text-accent-blue border border-accent-blue/30'
  }
  if (h.includes('perf') || h.includes('optim')) {
    return 'bg-accent-warning/15 text-accent-warning border border-accent-warning/30'
  }
  if (h.includes('break') || h.includes('deprecat')) {
    return 'bg-accent-red/15 text-accent-red border border-accent-red/30'
  }
  // Default
  return 'bg-surface-elevated text-text-secondary border border-border-default'
}

/**
 * Clean up heading text into a short badge label.
 */
function categoryLabel(heading: string): string {
  // If heading is already short enough, use as-is
  if (heading.length <= 20) return heading
  // Try to shorten common patterns
  const h = heading.toLowerCase()
  if (h.includes('feature')) return 'Features'
  if (h.includes('fix')) return 'Fixes'
  if (h.includes('improve')) return 'Improvements'
  if (h.includes('breaking')) return 'Breaking'
  return heading
}

onMounted(loadReleases)
</script>
