import { defineStore, acceptHMRUpdate } from "pinia";
import { useToastStore } from "./toastStore";
import type { WatchRuleTriggered } from "../types/database";

/**
 * Watchword alert store - handles audio and toast notifications
 * for watch-rule-triggered events.
 *
 * Registered as an always-on listener in startupStore so alerts
 * fire regardless of which screen is active.
 */

let audioContext: AudioContext | null = null;

/**
 * Play a short notification beep using the Web Audio API.
 * Creates an AudioContext lazily on first use (browsers require
 * a user gesture before the context can start, but Tauri desktop
 * apps don't have that restriction).
 */
function playNotificationBeep() {
  try {
    if (!audioContext) {
      audioContext = new AudioContext();
    }

    const ctx = audioContext;

    // Resume if suspended (can happen after inactivity)
    if (ctx.state === "suspended") {
      ctx.resume();
    }

    const now = ctx.currentTime;

    // Two-tone ascending beep for a pleasant notification sound
    // First tone: 880 Hz for 100ms
    const osc1 = ctx.createOscillator();
    const gain1 = ctx.createGain();
    osc1.type = "sine";
    osc1.frequency.value = 880;
    gain1.gain.setValueAtTime(0.3, now);
    gain1.gain.exponentialRampToValueAtTime(0.01, now + 0.1);
    osc1.connect(gain1);
    gain1.connect(ctx.destination);
    osc1.start(now);
    osc1.stop(now + 0.1);

    // Second tone: 1320 Hz for 150ms, starting 80ms after first
    const osc2 = ctx.createOscillator();
    const gain2 = ctx.createGain();
    osc2.type = "sine";
    osc2.frequency.value = 1320;
    gain2.gain.setValueAtTime(0, now + 0.08);
    gain2.gain.linearRampToValueAtTime(0.3, now + 0.1);
    gain2.gain.exponentialRampToValueAtTime(0.01, now + 0.25);
    osc2.connect(gain2);
    gain2.connect(ctx.destination);
    osc2.start(now + 0.08);
    osc2.stop(now + 0.25);
  } catch (e) {
    console.error("[watchword-alert] Failed to play notification beep:", e);
  }
}

export const useWatchwordAlertStore = defineStore("watchwordAlert", () => {
  function handleWatchRuleTriggered(payload: WatchRuleTriggered) {
    const { notify, rule_name, sender, message } = payload;

    if (notify.sound) {
      playNotificationBeep();
    }

    if (notify.toast) {
      const toastStore = useToastStore();
      const senderLabel = sender ? `[${sender}]` : "";
      // Truncate long messages for the toast
      const truncated =
        message.length > 80 ? message.substring(0, 77) + "..." : message;
      toastStore.add(
        "info",
        `Watchword "${rule_name}" ${senderLabel}: ${truncated}`,
        { action: { label: "Configure", view: "chat", subTab: "watchwords" } }
      );
    }
  }

  return {
    handleWatchRuleTriggered,
  };
});

if (import.meta.hot) {
  import.meta.hot.accept(acceptHMRUpdate(useWatchwordAlertStore, import.meta.hot));
}
