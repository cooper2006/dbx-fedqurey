<script setup lang="ts">
import { defineAsyncComponent } from "vue";
import { usePanelResize } from "@/composables/usePanelResize";
import type { HistoryEntry } from "@/lib/backend/tauri";

defineProps<{
  open: boolean;
  classicLayout: boolean;
}>();

const emit = defineEmits<{
  (e: "close"): void;
  (e: "restore", sql: string, entry: HistoryEntry): void;
  (e: "analyze-ai", entry: HistoryEntry): void;
}>();

const QueryHistory = defineAsyncComponent(() => import("@/components/editor/QueryHistory.vue"));

const { historyWidth, startHistoryResize } = usePanelResize();

function onRestore(sql: string, entry: HistoryEntry) {
  emit("restore", sql, entry);
}

function onAnalyzeAi(entry: HistoryEntry) {
  emit("analyze-ai", entry);
}
</script>

<template>
  <div v-if="open" :class="classicLayout ? 'h-full shrink-0 relative z-30 isolate bg-background' : 'h-full shrink-0 relative z-30 isolate rounded-md border border-border/80 bg-background'" :style="{ width: historyWidth + 'px' }">
    <div class="panel-resize-handle panel-resize-handle--left" @mousedown="startHistoryResize" />
    <QueryHistory @restore="onRestore" @analyze-ai="onAnalyzeAi" @close="emit('close')" />
  </div>
</template>
