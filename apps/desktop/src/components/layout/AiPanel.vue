<script setup lang="ts">
import { ref, defineAsyncComponent } from "vue";
import { usePanelResize } from "@/composables/usePanelResize";
import type { QueryTab, ConnectionConfig } from "@/types/database";
import type { AiAction } from "@/lib/ai/ai";

const props = defineProps<{
  open: boolean;
  classicLayout: boolean;
  activeTab: QueryTab | undefined;
  activeConnection: ConnectionConfig | undefined;
}>();

const emit = defineEmits<{
  (e: "close"): void;
  (e: "replace-sql", sql: string): void;
  (e: "execute-sql", sql: string): void;
  (e: "request-auto-execute-sql", sql: string): void;
  (e: "open-explain-plan", sql: string): void;
}>();

const AiAssistant = defineAsyncComponent(() => import("@/components/editor/AiAssistant.vue"));

type AiAssistantHandle = {
  triggerAction: (action: AiAction, instruction?: string) => void;
};

const aiPanelReady = ref(false);
const aiAssistantRef = ref<AiAssistantHandle | null>(null);
const { aiPanelWidth, startAiPanelResize } = usePanelResize();

function onAiReplaceSql(sql: string) {
  emit("replace-sql", sql);
}

function onAiExecuteSql(sql: string) {
  emit("execute-sql", sql);
}

function onAiRequestAutoExecuteSql(sql: string) {
  emit("request-auto-execute-sql", sql);
}

function onAiOpenExplainPlan(sql: string) {
  emit("open-explain-plan", sql);
}

function toggleAiPanel() {
  emit("close");
}

function triggerAction(action: AiAction, instruction?: string) {
  aiAssistantRef.value?.triggerAction(action, instruction);
}

defineExpose({ triggerAction });

if (typeof window !== "undefined") {
  requestAnimationFrame(() => {
    aiPanelReady.value = true;
  });
}
</script>

<template>
  <div v-if="open" :class="classicLayout ? 'h-full shrink-0 relative z-30 isolate bg-background' : 'h-full shrink-0 relative z-30 isolate rounded-md border border-border/80 bg-background'" :style="{ width: aiPanelWidth + 'px' }">
    <div class="panel-resize-handle panel-resize-handle--left" @mousedown="startAiPanelResize" />
    <div class="h-full min-h-0 overflow-hidden">
      <AiAssistant v-if="aiPanelReady" ref="aiAssistantRef" :tab="activeTab" :connection="activeConnection" @replace-sql="onAiReplaceSql" @execute-sql="onAiExecuteSql" @request-auto-execute-sql="onAiRequestAutoExecuteSql" @open-explain-plan="onAiOpenExplainPlan" @close="toggleAiPanel" />
    </div>
  </div>
</template>
