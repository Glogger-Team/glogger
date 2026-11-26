<template>
  <div class="fixed inset-0 bg-surface-dark flex flex-col items-center justify-center p-8">
    <div class="mb-8 text-center">
      <h1 class="h1-brand">glogger</h1>
      <p class="text-text-muted text-xs mt-1">First-Time Setup</p>
    </div>

    <!-- Step indicators -->
    <div class="flex items-center gap-2 mb-8">
      <div
        v-for="(label, i) in stepLabels"
        :key="i"
        class="flex items-center gap-2">
        <div
          class="w-8 h-8 rounded-full flex items-center justify-center text-xs font-bold border-2 transition-colors"
          :class="stepClass(i)">
          <span v-if="i < currentStep">&#10003;</span>
          <span v-else>{{ i + 1 }}</span>
        </div>
        <span
          class="text-xs hidden sm:inline transition-colors"
          :class="i === currentStep ? 'text-text-primary' : 'text-text-muted'">
          {{ label }}
        </span>
        <div v-if="i < stepLabels.length - 1" class="w-8 h-px bg-border-default mx-1" />
      </div>
    </div>

    <div class="w-full max-w-xl">
      <slot />
    </div>
  </div>
</template>

<script setup lang="ts">
const stepLabels = ["Game Folder", "Watchers", "Character"];

const props = defineProps<{
  currentStep: number;
}>();

function stepClass(index: number) {
  if (index < props.currentStep) {
    return "border-accent-green bg-accent-green/20 text-accent-green";
  }
  if (index === props.currentStep) {
    return "border-accent-gold bg-accent-gold/20 text-accent-gold";
  }
  return "border-border-default text-text-muted";
}
</script>
