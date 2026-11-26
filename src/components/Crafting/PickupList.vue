<template>
  <div class="flex flex-col gap-3">
    <div v-if="!bare" class="flex items-center justify-between">
      <h4 class="panel-label m-0">
        Pickup List
      </h4>
      <span class="text-text-muted text-[10px]">{{ areas.length }} area{{ areas.length !== 1 ? 's' : '' }} to visit</span>
    </div>

    <div v-if="areas.length === 0" class="text-text-dim text-xs italic">
      No items to pick up from storage vaults.
    </div>

    <div v-for="area in areas" :key="area.name" class="bg-surface-base border border-surface-elevated rounded p-3">
      <h5 class="text-accent-gold text-xs font-semibold m-0 mb-2">{{ area.name }}</h5>
      <ul class="list-none m-0 p-0 space-y-1">
        <li
          v-for="item in area.items"
          :key="`${item.vault_name}-${item.item_id}`"
          class="flex items-center gap-2 text-xs"
          :class="{ 'opacity-40 line-through': item.checked }"
          @click="item.checked = !item.checked">
          <input
            type="checkbox"
            :checked="item.checked"
            class="accent-accent-gold cursor-pointer"
            @click.stop="item.checked = !item.checked" />
          <ItemInline :reference="item.item_name" />
          <span class="text-text-muted text-[10px] ml-auto mr-1" :title="item.vault_name">
            {{ item.vault_label }}
          </span>
          <span class="text-text-primary shrink-0">
            ×{{ item.pickup_quantity }}
          </span>
          <span v-if="item.pickup_quantity < item.available" class="text-text-muted text-[10px]">
            ({{ item.available }} avail)
          </span>
        </li>
      </ul>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, reactive, onMounted, ref } from "vue";
import type { MaterialNeed } from "../../types/crafting";
import { useGameDataStore } from "../../stores/gameDataStore";
import ItemInline from "../Shared/Item/ItemInline.vue";

interface PickupItem {
  item_id: number
  item_name: string
  vault_name: string
  vault_label: string
  pickup_quantity: number
  available: number
  checked: boolean
}

interface AreaPickup {
  name: string
  items: PickupItem[]
}

const props = withDefaults(defineProps<{
  needs: MaterialNeed[]
  bare?: boolean
}>(), {
  bare: false,
});

const gameData = useGameDataStore();

// Map vault_key → area_name, loaded once
const vaultAreaMap = ref(new Map<string, string>());
const vaultLabelMap = ref(new Map<string, string>());

onMounted(async () => {
  const zones = await gameData.getStorageVaultZones();
  for (const z of zones) {
    vaultAreaMap.value.set(z.vault_key, z.area_name ?? "Unknown");
    vaultLabelMap.value.set(z.vault_key, z.npc_friendly_name ?? z.vault_key);
  }
});

const areas = computed(() => {
  const areaMap = new Map<string, PickupItem[]>();

  for (const mat of props.needs) {
    if (mat.storage_have === 0) continue;

    // Track how much we still need to pick up across all vault entries.
    // For shortfall items we only need what's missing; for fully-stocked
    // items we need the lesser of quantity_needed and what's in storage.
    // Subtract inventory_have since those don't need pickup.
    let remaining = Math.max(0, mat.quantity_needed - mat.inventory_have);

    for (const vs of mat.vault_breakdown) {
      if (vs.quantity === 0 || remaining <= 0) continue;

      const pickupQty = Math.min(vs.quantity, remaining);
      remaining -= pickupQty;

      const areaName = vaultAreaMap.value.get(vs.vault_name) ?? vs.vault_name;
      const vaultLabel = vaultLabelMap.value.get(vs.vault_name) ?? vs.vault_name;

      // For dynamic materials, use the concrete item info from the vault entry
      const itemId = (mat.is_dynamic && vs.item_id != null) ? vs.item_id : mat.item_id;
      const itemName = (mat.is_dynamic && vs.item_name) ? vs.item_name : mat.item_name;

      if (!areaMap.has(areaName)) {
        areaMap.set(areaName, []);
      }
      areaMap.get(areaName)!.push(reactive({
        item_id: itemId,
        item_name: itemName,
        vault_name: vs.vault_name,
        vault_label: vaultLabel,
        pickup_quantity: pickupQty,
        available: vs.quantity,
        checked: false,
      }));
    }
  }

  const result: AreaPickup[] = [];
  for (const [name, items] of areaMap) {
    items.sort((a, b) => a.item_name.localeCompare(b.item_name));
    result.push({ name, items });
  }
  result.sort((a, b) => a.name.localeCompare(b.name));
  return result;
});
</script>
