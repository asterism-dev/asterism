<script setup lang="ts">
import type { DockviewPanelApi } from 'dockview-vue';
import { onUnmounted, ref } from 'vue';
import ActivityView from '../ActivityView.vue';

const props = defineProps<{ params: { api: DockviewPanelApi } }>();
// dockview keeps background tabs mounted; unmounting the view stops its polling.
const visible = ref(props.params.api.isVisible);
const subscription = props.params.api.onDidVisibilityChange((e) => {
  visible.value = e.isVisible;
});
onUnmounted(() => subscription.dispose());
</script>

<template>
  <div class="pane"><ActivityView v-if="visible" /></div>
</template>
