<template>
  <div class="border border-surface-elevated rounded">
    <button
      class="w-full flex items-center justify-between px-3 py-2 bg-transparent border-none cursor-pointer text-left hover:bg-surface-elevated/50 transition-colors"
      @click="open = !open">
      <div class="flex items-center gap-2">
        <span class="text-text-secondary text-xs transition-transform" :class="{ 'rotate-90': open }">&#9654;</span>
        <h4 class="panel-label m-0">
          <slot name="title" />
        </h4>
      </div>
      <slot name="badge" />
    </button>
    <div v-show="open" class="px-3 pb-3">
      <slot />
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref } from "vue";

const props = withDefaults(defineProps<{
  defaultOpen?: boolean
}>(), {
  defaultOpen: true,
});

const open = ref(props.defaultOpen);
</script>
