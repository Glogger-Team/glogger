<template>
  <div v-if="loading" class="flex flex-col gap-1.5">
    <div class="text-[10px] leading-tight uppercase tracking-widest text-text-dim border-b border-surface-card pb-0.5">Sources</div>
    <SkeletonLoader variant="text" :lines="2" />
  </div>

  <div v-else-if="sources && hasSources" class="flex flex-col gap-1.5">
    <div class="text-[10px] leading-tight uppercase tracking-widest text-text-dim border-b border-surface-card pb-0.5">Sources</div>
    <div class="flex flex-col gap-1">
      <!-- CDN source entries -->
      <div
        v-for="(entry, i) in sources.cdn_sources"
        :key="'cdn-' + i"
        class="text-xs flex gap-1.5 px-2 py-0.5 items-center">
        <span class="text-[#6a8a6a] shrink-0">{{ sourceIcon(entry.source_type) }}</span>
        <SourceEntryRow :entry="entry" />
      </div>

      <!-- Items that bestow this entity -->
      <div
        v-for="item in sources.bestowed_by_items"
        :key="'item-' + item.id"
        class="text-xs flex gap-1.5 px-2 py-0.5 items-center">
        <span class="text-[#6a8a6a] shrink-0">&#128214;</span>
        <span class="text-text-secondary">Taught by item:</span>
        <ItemInline :reference="item.name" />
      </div>

      <!-- Quests that reward this entity -->
      <div
        v-for="quest in sources.rewarded_by_quests"
        :key="'quest-' + quest.key"
        class="text-xs flex gap-1.5 px-2 py-0.5 items-center">
        <span class="text-[#6a8a6a] shrink-0">&#10070;</span>
        <span class="text-text-secondary">Quest reward:</span>
        <QuestInline :reference="quest.key" />
      </div>
    </div>
  </div>

  <div v-else-if="sources && !hasSources" class="flex flex-col gap-1.5">
    <div class="text-[10px] leading-tight uppercase tracking-widest text-text-dim border-b border-surface-card pb-0.5">Sources</div>
    <div class="text-xs text-text-dim italic px-2 py-1">No known sources</div>
  </div>
</template>

<script setup lang="ts">
import { computed } from "vue";
import type { EntitySources } from "../../types/gameData";
import ItemInline from "./Item/ItemInline.vue";
import QuestInline from "./Quest/QuestInline.vue";
import SourceEntryRow from "./SourceEntryRow.vue";
import SkeletonLoader from "./SkeletonLoader.vue";

const props = defineProps<{
  sources: EntitySources | null
  loading?: boolean
}>();

const hasSources = computed(() => {
  if (!props.sources) return false;
  return (
    props.sources.cdn_sources.length > 0 ||
    props.sources.bestowed_by_items.length > 0 ||
    props.sources.rewarded_by_quests.length > 0
  );
});

function sourceIcon(type: string): string {
  switch (type) {
    case "Skill": return "\u2B06";
    case "Training": return "\uD83D\uDDE3";
    case "Vendor": return "\uD83D\uDCB0";
    case "Barter": return "\uD83E\uDD1D";
    case "Quest": return "\u2726";
    case "Effect": return "\u2728";
    case "Item": return "\uD83D\uDCE6";
    case "Recipe": return "\uD83D\uDD28";
    case "Monster": return "\uD83D\uDC80";
    case "NpcGift": return "\uD83C\uDF81";
    case "HangOut": return "\u2615";
    case "Angling": return "\uD83C\uDFA3";
    case "TreasureMap": return "\uD83D\uDDFA";
    case "CorpseButchering":
    case "CorpseSkinning":
    case "CorpseSkullExtraction":
      return "\uD83E\uDE78";
    case "ResourceInteractor":
    case "CraftedInteractor":
      return "\u2692";
    default: return "\u2022";
  }
}
</script>
