<template>
  <div v-if="showFederatedIndicator" class="federated-query-status-bar flex items-center gap-2 px-3 py-1 text-xs border-t border-border/50 bg-muted/30 shrink-0">
    <!-- 联邦查询状态图标 -->
    <Network v-if="isFederated" class="w-3.5 h-3.5 text-cyan-400 shrink-0" :title="t('federation.enabled')" />
    <Database v-else class="w-3.5 h-3.5 text-muted-foreground shrink-0" :title="t('federation.disabled')" />

    <!-- 状态文本 -->
    <span class="text-foreground font-medium">
      {{ statusText }}
    </span>

    <!-- 联邦连接信息 -->
    <template v-if="connections.length > 0">
      <Badge variant="secondary" class="text-xs"> {{ connections.length }} {{ t("federation.connections") }} </Badge>

      <!-- 连接列表标签 -->
      <div class="flex gap-1 flex-wrap">
        <Badge v-for="conn in connections" :key="conn" variant="outline" class="text-xs px-1.5 py-0">
          {{ conn }}
        </Badge>
      </div>
    </template>

    <!-- 提示图标（需要 Calcite Agent） -->
    <LightTooltip v-if="requiresCalcite && !isConnectedToCalcite" :content="t('federation.requiresCalcite')">
      <AlertCircle class="w-3.5 h-3.5 text-amber-500 cursor-help ml-auto shrink-0" />
    </LightTooltip>

    <!-- 单连接成功指示器 -->
    <CheckCircle2 v-if="isFederated && !requiresCalcite" class="w-3.5 h-3.5 text-green-500 ml-auto shrink-0" />

    <!-- 联邦语法提示 -->
    <span v-if="!isFederated" class="text-muted-foreground ml-auto">
      {{ t("federation.syntaxHint") }}
    </span>
  </div>
</template>

<script setup lang="ts">
import { computed } from "vue";
import { useI18n } from "vue-i18n";
import { Network, Database, AlertCircle, CheckCircle2 } from "@lucide/vue";
import { Badge } from "@/components/ui/badge";
import LightTooltip from "@/components/ui/LightTooltip.vue";
import { analyzeFederatedSql } from "@/lib/federated/federatedFormatter";
import { useConnectionStore } from "@/stores/connectionStore";

const props = defineProps<{
  sql: string;
  connectionId?: string;
}>();

const { t } = useI18n();
const connectionStore = useConnectionStore();

// 分析 SQL 是否为联邦查询
const analysis = computed(() => analyzeFederatedSql(props.sql));

// 是否使用联邦语法
const isFederated = computed(() => analysis.value.usesFederation);

// 检测是否需要 Calcite Agent（多连接查询）
const requiresCalcite = computed(() => {
  return isFederated.value && analysis.value.connections.length > 1;
});

// 检查是否已连接到 Calcite Agent
const isConnectedToCalcite = computed(() => {
  // TODO: 实现 Calcite Agent 连接状态检查
  return false;
});

// 获取相关连接列表
const connections = computed(() => {
  if (!isFederated.value) return [];
  return analysis.value.connections;
});

// 状态文本
const statusText = computed(() => {
  if (isFederated.value) {
    if (connections.value.length === 1) {
      return t("federation.singleConnection");
    } else if (connections.value.length > 1) {
      return t("federation.multiConnection");
    }
  }
  return "";
});

// 是否显示状态栏（仅当当前连接启用了联邦，或 SQL 中使用了联邦语法时显示）
const showFederatedIndicator = computed(() => {
  if (isFederated.value) return true;
  // 检查当前连接是否启用了联邦
  if (props.connectionId) {
    const config = connectionStore.getConfig(props.connectionId);
    return !!config?.federation_enabled;
  }
  return false;
});
</script>

<script lang="ts">
export default {
  name: "FederatedQueryStatusBar",
};
</script>

<style scoped>
.federated-query-status-bar {
  min-height: 28px;
}
</style>
