<template>
  <div class="fixed inset-0 bg-surface-dark flex items-center justify-center">
    <div class="w-full max-w-md px-8">
      <h1 class="h1-brand mb-2 text-center">glogger</h1>
      <p class="text-text-muted text-sm mb-8 text-center">Starting up...</p>

      <div class="space-y-3">
        <div
          v-for="(task, i) in tasks"
          :key="i"
          class="flex items-center gap-3 text-sm">
          <!-- Status icon -->
          <div class="w-5 h-5 flex items-center justify-center flex-shrink-0">
            <div v-if="task.status === 'pending'" class="w-2 h-2 rounded-full bg-border-default" />
            <div v-else-if="task.status === 'running'" class="w-4 h-4 border-2 border-accent-gold/30 border-t-accent-gold rounded-full animate-spin" />
            <span v-else-if="task.status === 'done'" class="text-accent-green">&#10003;</span>
            <span v-else-if="task.status === 'error'" class="text-accent-red">&#10007;</span>
          </div>

          <div>
            <span :class="task.status === 'pending' ? 'text-text-muted' : task.status === 'error' ? 'text-accent-red' : 'text-text-primary'">
              {{ task.label }}
            </span>
            <div v-if="task.detail" class="text-text-muted text-xs mt-0.5">{{ task.detail }}</div>
          </div>
        </div>
      </div>

      <div v-if="hasError" class="mt-8 p-4 bg-accent-red/10 border border-accent-red/30 rounded text-sm text-text-primary">
        <p class="font-medium text-accent-red mb-1">Startup failed</p>
        <p class="text-text-muted">{{ errorMessage || 'A required startup task failed. Please restart the application.' }}</p>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed } from "vue";
import type { StartupTask } from "../../stores/startupStore";

const props = defineProps<{
  tasks: StartupTask[];
  errorMessage?: string | null;
}>();

const hasError = computed(() => props.tasks.some(t => t.status === "error"));
</script>
