<template>
  <Teleport to="body">
    <div class="fixed bottom-4 right-4 z-[100] flex flex-col gap-2">
      <TransitionGroup
        enter-active-class="transition-all duration-300 ease-out"
        leave-active-class="transition-all duration-200 ease-in"
        enter-from-class="opacity-0 translate-x-full"
        leave-to-class="opacity-0 translate-x-full">
        <div
          v-for="toast in store.visibleToasts"
          :key="toast.id"
          class="max-w-[min(350px,calc(100vw-2rem))] bg-surface-elevated border border-border-default rounded text-xs flex items-start gap-2 px-3 py-2 shadow-lg border-l-2"
          :class="borderClass(toast.type)"
          @mouseenter="pauseTimer(toast.id)"
          @mouseleave="resumeTimer(toast)">
          <span class="shrink-0" :class="prefixClass(toast.type)">{{ prefix(toast.type) }}</span>
          <span class="flex-1 text-text-primary">{{ toast.message }}</span>
          <button
            class="shrink-0 text-text-muted hover:text-text-primary cursor-pointer bg-transparent border-none text-xs"
            @click="store.dismiss(toast.id)">
            ✕
          </button>
        </div>
      </TransitionGroup>
    </div>
  </Teleport>
</template>

<script setup lang="ts">
import { onBeforeUnmount } from "vue";
import { useToastStore, type ToastType, type Toast } from "../../stores/toastStore";

const store = useToastStore();

const timers = new Map<number, ReturnType<typeof setTimeout>>();

function borderClass(type: ToastType): string {
  switch (type) {
    case "success": return "border-l-accent-green";
    case "info": return "border-l-accent-blue";
    case "warning": return "border-l-accent-warning";
    case "error": return "border-l-accent-red";
  }
}

function prefixClass(type: ToastType): string {
  switch (type) {
    case "success": return "text-accent-green";
    case "info": return "text-accent-blue";
    case "warning": return "text-accent-warning";
    case "error": return "text-accent-red";
  }
}

function prefix(type: ToastType): string {
  switch (type) {
    case "success": return "✓";
    case "info": return "●";
    case "warning": return "▲";
    case "error": return "✕";
  }
}

function startTimer(toast: Toast) {
  if (!toast.autoDismiss) return;
  clearTimer(toast.id);
  const timer = setTimeout(() => {
    store.dismiss(toast.id);
    timers.delete(toast.id);
  }, store.AUTO_DISMISS_MS);
  timers.set(toast.id, timer);
}

function clearTimer(id: number) {
  const existing = timers.get(id);
  if (existing) {
    clearTimeout(existing);
    timers.delete(id);
  }
}

function pauseTimer(id: number) {
  clearTimer(id);
}

function resumeTimer(toast: Toast) {
  startTimer(toast);
}

// Watch for new toasts to start their timers
import { watch } from "vue";
watch(
  () => store.visibleToasts,
  (toasts) => {
    for (const toast of toasts) {
      if (!timers.has(toast.id) && toast.autoDismiss) {
        startTimer(toast);
      }
    }
  },
  { immediate: true, deep: true }
);

onBeforeUnmount(() => {
  for (const timer of timers.values()) {
    clearTimeout(timer);
  }
  timers.clear();
});
</script>
