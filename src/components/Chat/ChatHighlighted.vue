<template>
  <span>
    <template v-for="(part, index) in parts" :key="index">
      <mark v-if="part.matched" class="bg-accent-gold/30 text-text-primary rounded-sm px-0.5">{{ part.text }}</mark>
      <template v-else>{{ part.text }}</template>
    </template>
  </span>
</template>

<script setup lang="ts">
import { computed } from 'vue'

/** One highlight term + the backend FTS match kind it carries.
 *  - `exact`: matches whole tokens only (word boundary)
 *  - `phrase`: contiguous substring (FTS5 phrases are token sequences;
 *    substring is the closest DOM approximation)
 *  - `prefix`: must start a token (FTS5 `term*`)
 *  - `literal`: malformed token — substring, mirrors the LIKE fallback */
export interface HighlightTerm {
  term: string
  kind: 'exact' | 'phrase' | 'prefix' | 'literal'
}

const props = defineProps<{
  message: string
  /** Search terms to highlight with their FTS match kinds */
  terms: HighlightTerm[]
}>()

interface HighlightPart {
  text: string
  matched: boolean
}

/** FTS5 token characters under the default unicode61 tokenizer: letter and
 *  number runs. `_` is a SEPARATOR in unicode61 (not part of tokens), so
 *  FTS matches `foo` inside `foo_bar` — the boundary class must match.
 *  Folding (NFD + stripping `\p{M}` marks) never removes or adds `\p{L}`/
 *  `\p{N}` characters, so `_` handling and token-boundary semantics are
 *  unchanged when matching the folded string. */
const TOKEN_CHAR = '[\\p{L}\\p{N}]'

/** Accent-folded form of a string: NFD decomposes accented letters, then
 *  combining marks are dropped. Mirrors FTS5 unicode61's default
 *  `remove_diacritics=1` closely enough that MATCH 'cafe' hits 'café'. */
function foldAccent(s: string): string {
  return s.normalize('NFD').replace(/\p{M}/gu, '')
}

/** Fold the message while recording, for each ORIGINAL code point, the
 *  folded position where its folded form ENDS (`ends[i]`, exclusive).
 *  A code point whose folded form is empty (a stripped combining mark)
 *  shares its end with the previous one; one whose folded form is longer
 *  than itself (Hangul NFD decomposition) spans several folded positions.
 *  Folded match boundaries convert back through `ends` to ORIGINAL
 *  code-point indices: start → first `i` with `ends[i] > pos`; end → first
 *  `i` with `ends[i] >= end` (its index + 1). Folding per CODE POINT keeps
 *  the map 1:1 with `[...message]`, so index mapping is surrogate-safe. */
function foldWithIndexMap(message: string): {
  folded: string,
  ends: number[],
} {
  let folded = ''
  const ends: number[] = []
  for (const ch of message) {
    folded += foldAccent(ch)
    ends.push(folded.length)
  }
  return { folded, ends }
}

const parts = computed((): HighlightPart[] => {
  const patterns: string[] = []
  for (const { term, kind } of props.terms) {
    if (term.length === 0) continue
    const esc = RegExp.escape(foldAccent(term))
    switch (kind) {
      case 'exact':
        // Whole token(s): no token chars touching the match on either side
        patterns.push(`(?<!${TOKEN_CHAR})${esc}(?!${TOKEN_CHAR})`)
        break
      case 'prefix':
        // Match must START a token; the token may continue after it
        patterns.push(`(?<!${TOKEN_CHAR})${esc}`)
        break
      case 'phrase':
      case 'literal':
        // Contiguous substring (phrases span token boundaries by design;
        // literals mirror the LIKE fallback)
        patterns.push(esc)
        break
    }
  }
  if (patterns.length === 0) {
    return [{ text: props.message, matched: false }]
  }

  // Match on the accent-folded message with accent-folded terms (FTS5
  // unicode61 default remove_diacritics=1 folds diacritics both ways),
  // then map match boundaries back to original code-point indices through
  // the per-code-point folded-end map and slice the ORIGINAL string with
  // Array.from (code-point units — JS String indices would split
  // surrogates, e.g. an emoji adjacent to a match).
  const { folded, ends } = foldWithIndexMap(props.message)
  const regex = new RegExp(`(${patterns.join('|')})`, 'giu')
  const chars = Array.from(props.message)
  const result: HighlightPart[] = []
  let lastIndex = 0
  for (const m of folded.matchAll(regex)) {
    const pos = m.index ?? 0
    const end = pos + m[0].length
    // Folded boundary → original code-point index: the first code point
    // whose folded form ends strictly after the boundary (its index; a
    // stripped zero-width mark shares ends with its neighbor, so a mark
    // directly before the match is skipped rather than swallowed).
    const coverStart = ends.findIndex(e => e > pos)
    // Match end → index+1 of the first code point whose folded form
    // reaches `end` (code points spanning the boundary — e.g. Hangul NFD
    // expansion — are fully covered).
    const coverEnd = ends.findIndex(e => e >= end)
    const origStart = coverStart !== -1 ? coverStart : chars.length
    let origEnd = coverEnd !== -1 ? coverEnd + 1 : chars.length
    // Trailing zero-width code points (stripped combining marks) belong to
    // the matched grapheme — `é` written as e + U+0301 would otherwise be
    // split into matched "e" + unmatched mark. Extend while the next code
    // point shares the last covered folded end.
    while (origEnd < chars.length && ends[origEnd] === ends[origEnd - 1]) {
      origEnd++
    }
    if (origStart > lastIndex) {
      result.push({ text: chars.slice(lastIndex, origStart).join(''), matched: false })
    }
    result.push({ text: chars.slice(origStart, origEnd).join(''), matched: true })
    lastIndex = origEnd
  }
  if (lastIndex < chars.length) {
    result.push({ text: chars.slice(lastIndex).join(''), matched: false })
  }
  return result.length > 0 ? result : [{ text: props.message, matched: false }]
})
</script>
