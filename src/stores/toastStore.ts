import { defineStore, acceptHMRUpdate } from "pinia";
import { ref, computed } from "vue";

export type ToastType = "success" | "info" | "warning" | "error";

export interface ToastAction {
  label: string;
  view: string;
  subTab?: string;
}

export interface Toast {
  id: number;
  type: ToastType;
  message: string;
  createdAt: number;
  autoDismiss: boolean;
  action?: ToastAction;
}

const MAX_VISIBLE = 3;
const AUTO_DISMISS_MS = 4000;

let nextId = 1;

export const useToastStore = defineStore("toast", () => {
  const toasts = ref<Toast[]>([]);

  const visibleToasts = computed(() => toasts.value.slice(0, MAX_VISIBLE));

  function add(type: ToastType, message: string, opts?: { action?: ToastAction }): number {
    const id = nextId++;
    const toast: Toast = {
      id,
      type,
      message,
      createdAt: Date.now(),
      autoDismiss: type !== "error",
      ...(opts?.action ? { action: opts.action } : {}),
    };
    // Prepend newest on top
    toasts.value = [toast, ...toasts.value];

    // Auto-dismiss overflow (oldest beyond max)
    if (toasts.value.length > MAX_VISIBLE) {
      const overflow = toasts.value.slice(MAX_VISIBLE);
      for (const t of overflow) {
        dismiss(t.id);
      }
    }

    return id;
  }

  function dismiss(id: number) {
    toasts.value = toasts.value.filter((t) => t.id !== id);
  }

  function dismissTop() {
    if (toasts.value.length > 0) {
      dismiss(toasts.value[0].id);
    }
  }

  return {
    toasts,
    visibleToasts,
    add,
    dismiss,
    dismissTop,
    AUTO_DISMISS_MS,
  };
});

if (import.meta.hot) {
  import.meta.hot.accept(acceptHMRUpdate(useToastStore, import.meta.hot));
}
