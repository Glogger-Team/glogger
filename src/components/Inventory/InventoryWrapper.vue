<template>
  <PaneLayout screen-key="inventory">
    <div class="flex flex-col gap-4 h-full">
      <div class="flex-1 min-h-0">
        <InventoryView v-if="activeTab === 'storage'" />
        <VaultDatabaseTab v-else-if="activeTab === 'vaults'" />
        <ConsolidateTab v-else-if="activeTab === 'consolidate'" />
      </div>
    </div>
  </PaneLayout>
</template>

<script setup lang="ts">
import PaneLayout from '../Shared/PaneLayout.vue'

import InventoryView from '../Character/InventoryView.vue'
import VaultDatabaseTab from './VaultDatabaseTab.vue'
import ConsolidateTab from './ConsolidateTab.vue'

defineProps<{
  activeTab: string;
}>();
</script>
