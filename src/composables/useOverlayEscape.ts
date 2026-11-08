import { onBeforeUnmount, watch } from "vue";

// Shared Esc stack for overlays: one Esc with stacked overlays closes only
// the topmost, not both. Sibling capture-phase window listeners all fire for
// one keypress (stopPropagation skips other nodes, not same-target siblings),
// so stacked overlays must not own Esc listeners. Overlays register a closer
// while open; one shared capture-phase listener invokes only the topmost
// entry. Registration order = stacking order (newest on top).
type EscapeCloser = () => void;

const stack: { close: EscapeCloser }[] = [];

function onKeydown(event: KeyboardEvent): void {
  if (event.key !== "Escape" || stack.length === 0) return;
  event.preventDefault();
  event.stopPropagation();
  // Same-target siblings still fire after stopPropagation: suppress listeners
  // registered after us so the overlay underneath does not close too.
  event.stopImmediatePropagation();
  stack[stack.length - 1].close();
}

// Eager module-level attach: imports evaluate before any component
// setup/mount, so this listener precedes every component-owned window
// listener. A stacked overlay therefore preempts legacy Esc listeners
// underneath it (e.g. QuickSearch sits above the picker dialogs).
// globalThis guard: HMR re-executes the module without reloading the page.
const INSTALLED_KEY = "__gloggerOverlayEscapeInstalled";
if (
  typeof window !== "undefined" &&
  !(globalThis as Record<string, unknown>)[INSTALLED_KEY]
) {
  (globalThis as Record<string, unknown>)[INSTALLED_KEY] = true;
  window.addEventListener("keydown", onKeydown, true);
}

export function registerOverlayEscape(close: EscapeCloser): () => void {
  const entry = { close };
  stack.push(entry);
  let done = false;
  return () => {
    if (done) return;
    done = true;
    const index = stack.indexOf(entry);
    if (index !== -1) stack.splice(index, 1);
  };
}

// Registers `close` while `isOpen()` is true; unregisters on close/unmount.
// `close` must be stable for the component lifetime.
export function useOverlayEscape(isOpen: () => boolean, close: EscapeCloser): void {
  let unregister: (() => void) | null = null;
  const stop = watch(
    isOpen,
    (open) => {
      unregister?.();
      unregister = open ? registerOverlayEscape(close) : null;
    },
    { immediate: true },
  );
  onBeforeUnmount(() => {
    unregister?.();
    unregister = null;
    stop();
  });
}
