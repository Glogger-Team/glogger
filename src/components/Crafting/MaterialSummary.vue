<template>
  <div class="flex flex-col gap-3">
    <div v-if="!bare" class="flex items-center justify-between">
      <h4 class="panel-label m-0">
        Material Availability
      </h4>
      <div class="flex gap-3 text-[0.65rem] text-text-muted">
        <span>
          <span class="text-green-400">{{ coveredCount }}</span> covered
        </span>
        <span>
          <span class="text-yellow-400">{{ partialCount }}</span> partial
        </span>
        <span>
          <span class="text-accent-red">{{ missingCount }}</span> missing
        </span>
      </div>
    </div>

    <!-- Intermediates section (craftable items still in the material list) -->
    <div v-if="intermediateNeeds.length > 0" class="flex flex-col gap-1">
      <h5 class="text-text-dim text-[0.65rem] font-semibold uppercase tracking-wide m-0 px-0.5">
        Intermediates
        <span class="text-text-muted font-normal normal-case tracking-normal ml-1">{{ intermediateNeeds.length }}</span>
      </h5>
      <MaterialTable
        :items="intermediateNeeds"
        :pricing-mode="pricingMode"
        :customer-provides="customerProvides"
        :expanded-keys="expandedKeys"
        @toggle-expanded="toggleExpanded"
        @update-customer-provides="(key, qty) => emit('update-customer-provides', key, qty)" />
    </div>

    <!-- Raw materials section -->
    <div v-if="rawNeeds.length > 0" class="flex flex-col gap-1">
      <h5
        v-if="intermediateNeeds.length > 0"
        class="text-text-dim text-[0.65rem] font-semibold uppercase tracking-wide m-0 px-0.5">
        Raw Materials
        <span class="text-text-muted font-normal normal-case tracking-normal ml-1">{{ rawNeeds.length }}</span>
      </h5>
      <MaterialTable
        :items="rawNeeds"
        :pricing-mode="pricingMode"
        :customer-provides="customerProvides"
        :expanded-keys="expandedKeys"
        @toggle-expanded="toggleExpanded"
        @update-customer-provides="(key, qty) => emit('update-customer-provides', key, qty)" />
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, ref } from "vue";
import type { MaterialNeed } from "../../types/crafting";
import MaterialTable from "./MaterialTable.vue";

const props = withDefaults(defineProps<{
  needs: MaterialNeed[]
  bare?: boolean
  pricingMode?: boolean
  customerProvides?: Record<string, number>
}>(), {
  bare: false,
  pricingMode: false,
});

const emit = defineEmits<{
  'update-customer-provides': [key: string, quantity: number]
}>();

const expandedKeys = ref(new Set<string>());

function toggleExpanded(key: string) {
  const next = new Set(expandedKeys.value);
  if (next.has(key)) {
    next.delete(key);
  } else {
    next.add(key);
  }
  expandedKeys.value = next;
}

/** Intermediates: craftable items that appear in the material needs list */
const intermediateNeeds = computed(() =>
  props.needs.filter((m) => m.is_craftable && !m.is_dynamic),
);

/** Raw materials: non-craftable items plus dynamic/keyword slots */
const rawNeeds = computed(() =>
  props.needs.filter((m) => !m.is_craftable || m.is_dynamic),
);

const coveredCount = computed(() => props.needs.filter((m) => m.shortfall === 0).length);
const partialCount = computed(() =>
  props.needs.filter((m) => m.shortfall > 0 && (m.inventory_have > 0 || m.storage_have > 0)).length
);
const missingCount = computed(() =>
  props.needs.filter((m) => m.shortfall > 0 && m.inventory_have === 0 && m.storage_have === 0).length
);
</script>
