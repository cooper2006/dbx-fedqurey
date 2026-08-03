<script setup lang="ts">
import { defineAsyncComponent } from "vue";
import { usePanelResize } from "@/composables/usePanelResize";

defineProps<{
  open: boolean;
  classicLayout: boolean;
}>();

const emit = defineEmits<{
  (e: "close"): void;
}>();

const SqlLibraryPanel = defineAsyncComponent(() => import("@/components/layout/SqlLibraryPanel.vue"));

const { sqlLibraryWidth, startSqlLibraryResize } = usePanelResize();

function toggleSqlLibrary() {
  emit("close");
}
</script>

<template>
  <div v-if="open" :class="classicLayout ? 'h-full shrink-0 relative z-30 isolate bg-background' : 'h-full shrink-0 relative z-30 isolate rounded-md border border-border/80 bg-background'" :style="{ width: sqlLibraryWidth + 'px' }">
    <div class="panel-resize-handle panel-resize-handle--left" @mousedown="startSqlLibraryResize" />
    <div class="h-full min-h-0 overflow-hidden">
      <SqlLibraryPanel @close="toggleSqlLibrary" />
    </div>
  </div>
</template>
