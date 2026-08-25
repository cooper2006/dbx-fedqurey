<script setup lang="ts">
import { computed } from "vue";
import { Loader2 } from "@lucide/vue";
import { useI18n } from "vue-i18n";
import { queryExecutionLabelKey } from "@/lib/sql/queryExecutionState";
import type { QueryTab } from "@/types/database";

const props = defineProps<{
  tab: Pick<QueryTab, "mode" | "isExecuting" | "isCancelling">;
}>();

const { t } = useI18n();
const status = computed(() => {
  if (props.tab.mode !== "query" || !props.tab.isExecuting) return undefined;
  return props.tab.isCancelling ? "cancelling" : "running";
});
</script>

<template>
  <span
    v-if="status"
    data-tab-execution-status
    role="status"
    class="inline-flex h-4 w-4 shrink-0 items-center justify-center rounded-full bg-background/80"
    :class="status === 'cancelling' ? 'text-amber-600 dark:text-amber-400' : 'text-blue-600 dark:text-blue-400'"
    :aria-label="t(queryExecutionLabelKey(tab))"
    :title="t(queryExecutionLabelKey(tab))"
  >
    <Loader2 aria-hidden="true" class="h-3 w-3 animate-spin motion-reduce:animate-none" />
  </span>
</template>
