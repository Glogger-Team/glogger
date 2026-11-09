<template>
  <div
    ref="chipEl"
    class="relative inline-flex items-center gap-1 pl-2 pr-1 py-0.5 rounded text-xs font-medium cursor-pointer select-none shrink-0"
    :class="colorClasses"
    @mouseenter="onChipEnter"
    @mouseleave="onChipLeave"
    @click="handleClick"
  >
    <span class="truncate max-w-40">{{ pin.label }}</span>
    <button
      class="m-0.5 pl-1 pr-1 size-4 inline-flex items-center justify-center rounded-full text-[10px] leading-none text-current/50 hover:text-current bg-current/10 hover:bg-current/20 transition-colors"
      title="Unpin"
      @click.stop="handleUnpin"
    >X</button>
  </div>

  <Teleport to="body">
    <div
      v-if="showTooltip"
      ref="shelfTooltipEl"
      class="fixed z-[9999] min-w-62 max-w-87 bg-surface-card border rounded-md p-3 shadow-lg pointer-events-none"
      :class="borderClass"
      :style="tooltipStyle"
    >
      <component :is="tooltipComponent" v-if="tooltipComponent && tooltipData" v-bind="tooltipProps" />
      <div v-else class="text-text-muted text-xs">Loading...</div>
    </div>
  </Teleport>
</template>

<script setup lang="ts">
import { ref, computed, shallowRef, watch, nextTick, type CSSProperties, type Component } from "vue";
import { useGameDataStore } from "../../../stores/gameDataStore";
import { useReferenceShelfStore, type PinnedEntity } from "../../../stores/referenceShelfStore";
import { useEntityNavigation, type EntityType } from "../../../composables/useEntityNavigation";
import { useTooltip } from "../../../composables/useTooltip";
import { convertFileSrc } from "@tauri-apps/api/core";

// Lazy imports for tooltip components
import ItemTooltip from "../Item/ItemTooltip.vue";
import SkillTooltip from "../Skill/SkillTooltip.vue";
import NpcTooltip from "../NPC/NpcTooltip.vue";
import QuestTooltip from "../Quest/QuestTooltip.vue";
import RecipeTooltip from "../Recipe/RecipeTooltip.vue";
import AbilityTooltip from "../Ability/AbilityTooltip.vue";

const props = defineProps<{
  pin: PinnedEntity;
}>();

const store = useGameDataStore();
const shelf = useReferenceShelfStore();
const { navigateToEntity } = useEntityNavigation();

const chipEl = ref<HTMLElement | null>(null);
const chipRect = ref<DOMRect | null>(null);
const tooltipData = shallowRef<any>(null);
const tooltipIconSrc = ref<string | null>(null);

const {
  showTooltip,
  onMouseEnter: baseEnter,
  onMouseLeave: onChipLeave,
} = useTooltip({
  delay: 300,
  onHover: loadTooltipData,
});

function onChipEnter() {
  if (chipEl.value) {
    chipRect.value = chipEl.value.getBoundingClientRect();
  }
  updateTooltipPosition();
  baseEnter();
}

// Position tooltip above the chip, flipping below if not enough space above
const shelfTooltipEl = ref<HTMLElement | null>(null);
const tooltipStyle = ref<CSSProperties>({});

function updateTooltipPosition() {
  if (!chipRect.value) {
    tooltipStyle.value = {};
    return;
  }
  const rect = chipRect.value;
  const gap = 8;
  const tooltipHeight = shelfTooltipEl.value?.offsetHeight ?? 200;
  const tooltipWidth = shelfTooltipEl.value?.offsetWidth ?? 300;
  const spaceAbove = rect.top - gap;
  const spaceBelow = window.innerHeight - rect.bottom - gap;
  const openBelow = spaceAbove < tooltipHeight && spaceBelow > spaceAbove;
  const left = Math.max(4, Math.min(rect.left, window.innerWidth - tooltipWidth - 4));

  tooltipStyle.value = {
    left: `${left}px`,
    ...(openBelow
      ? { top: `${rect.bottom + gap}px` }
      : { bottom: `${window.innerHeight - rect.top + gap}px` }),
  };
}

watch(showTooltip, (visible) => {
  if (visible) {
    nextTick(updateTooltipPosition);
  }
});

const entityColorMap: Record<EntityType, string> = {
  item: "text-entity-item bg-entity-item/10 border border-entity-item/25 hover:bg-entity-item/20",
  skill: "text-entity-skill bg-entity-skill/10 border border-entity-skill/25 hover:bg-entity-skill/20",
  npc: "text-entity-npc bg-entity-npc/10 border border-entity-npc/25 hover:bg-entity-npc/20",
  quest: "text-entity-quest bg-entity-quest/10 border border-entity-quest/25 hover:bg-entity-quest/20",
  recipe: "text-entity-recipe bg-entity-recipe/10 border border-entity-recipe/25 hover:bg-entity-recipe/20",
  ability: "text-entity-ability bg-entity-ability/10 border border-entity-ability/25 hover:bg-entity-ability/20",
  area: "text-entity-area bg-entity-area/10 border border-entity-area/25 hover:bg-entity-area/20",
  enemy: "text-entity-enemy bg-entity-enemy/10 border border-entity-enemy/25 hover:bg-entity-enemy/20",
};

const entityBorderMap: Record<EntityType, string> = {
  item: "border-entity-item/50",
  skill: "border-entity-skill/50",
  npc: "border-entity-npc/50",
  quest: "border-entity-quest/50",
  recipe: "border-entity-recipe/50",
  ability: "border-entity-ability/50",
  area: "border-entity-area/50",
  enemy: "border-entity-enemy/50",
};

const colorClasses = computed(() => entityColorMap[props.pin.type]);

const borderClass = computed(() => entityBorderMap[props.pin.type]);

const tooltipComponent = computed<Component | null>(() => {
  const map: Partial<Record<EntityType, Component>> = {
    item: ItemTooltip,
    skill: SkillTooltip,
    npc: NpcTooltip,
    quest: QuestTooltip,
    recipe: RecipeTooltip,
    ability: AbilityTooltip,
  };
  return map[props.pin.type] ?? null;
});

const tooltipProps = computed(() => {
  if (!tooltipData.value) return {};
  const t = props.pin.type;
  if (t === "item") return { item: tooltipData.value, iconSrc: tooltipIconSrc.value };
  if (t === "skill") return { skill: tooltipData.value, iconSrc: tooltipIconSrc.value };
  if (t === "npc") return { npc: tooltipData.value };
  if (t === "quest") return { quest: tooltipData.value };
  if (t === "recipe") return { recipe: tooltipData.value, iconSrc: tooltipIconSrc.value };
  if (t === "ability") return { ability: tooltipData.value, iconSrc: tooltipIconSrc.value };
  return {};
});

async function loadTooltipData() {
  const t = props.pin.type;
  const ref = props.pin.reference;
  try {
    let data: any = null;
    if (t === "item") data = await store.resolveItem(ref);
    else if (t === "skill") data = await store.resolveSkill(ref);
    else if (t === "npc") data = store.resolveNpcSync(ref);
    else if (t === "quest") data = await store.resolveQuest(ref);
    else if (t === "recipe") data = await store.resolveRecipe(ref);
    else if (t === "ability") data = await store.resolveAbility(ref);

    if (!data) return;
    tooltipData.value = data;

    if (data.icon_id) {
      const path = await store.getIconPath(data.icon_id);
      tooltipIconSrc.value = convertFileSrc(path);
    }
  } catch (e) {
    console.warn(`Failed to load tooltip data for ${t}: ${ref}`, e);
  }
}

function handleClick() {
  navigateToEntity({ type: props.pin.type, id: props.pin.reference });
}

function handleUnpin() {
  shelf.unpin(props.pin.type, props.pin.reference);
}
</script>
