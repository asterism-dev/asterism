<script setup lang="ts">
import 'dockview-vue/dist/styles/dockview.css';
import { DockviewVue, themeLight, type DockviewReadyEvent, type VueComponent } from 'dockview-vue';
import { onUnmounted } from 'vue';
import { attachOuter, detachOuter } from '../dock/outer';
import FixedTab from './FixedTab.vue';
import ActivityPane from './panes/ActivityPane.vue';
import DiffPane from './panes/DiffPane.vue';
import ProjectsPane from './panes/ProjectsPane.vue';
import WorkspacePane from './panes/WorkspacePane.vue';

// dockview types panel components as prop-less; ours take its `params` prop.
const components = { projects: ProjectsPane, workspace: WorkspacePane, diff: DiffPane, activity: ActivityPane } as unknown as Record<string, VueComponent>;
const tabComponents = { fixed: FixedTab } as unknown as Record<string, VueComponent>;

onUnmounted(detachOuter);
</script>

<template>
  <DockviewVue
    class="dock"
    :theme="themeLight"
    :components="components"
    :tab-components="tabComponents"
    :disable-floating-groups="true"
    @ready="(e: DockviewReadyEvent) => attachOuter(e.api)"
  />
</template>
