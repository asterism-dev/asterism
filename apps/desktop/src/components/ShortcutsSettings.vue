<script setup lang="ts">
import { onUnmounted, ref } from 'vue';
import {
  ACTION_IDS,
  ACTIONS,
  IS_MAC,
  bindingFor,
  bindingFromEvent,
  conflictFor,
  defaultBinding,
  formatBinding,
  hasOverride,
  isUsable,
  resetAll,
  resetBinding,
  resetConflict,
  setBinding,
  shortcutHint,
  terminalSafe,
  type ActionId,
  type Binding,
} from '../shortcuts';

const editing = ref<ActionId | null>(null);
const notice = ref<string | null>(null);
const pending = ref<{ id: ActionId; binding: Binding; conflict: ActionId } | null>(null);

function inactiveInTerminal(id: ActionId) {
  const b = bindingFor(id);
  return id !== 'confirm' && b !== null && !terminalSafe(b, IS_MAC);
}

function startRecording(id: ActionId) {
  stopRecording();
  pending.value = null;
  editing.value = id;
  // Capture phase + stopPropagation keeps the global dispatcher in App.vue from acting on the keys.
  window.addEventListener('keydown', onRecordKey, true);
}

function stopRecording() {
  editing.value = null;
  notice.value = null;
  window.removeEventListener('keydown', onRecordKey, true);
}

function onRecordKey(e: KeyboardEvent) {
  e.preventDefault();
  e.stopPropagation();
  const id = editing.value;
  if (!id || e.key === 'Escape') return stopRecording();
  const b = bindingFromEvent(e);
  if (!b) return;
  if (!isUsable(b)) {
    notice.value = IS_MAC ? 'Include ⌘, ⌃ or ⌥.' : 'Include Ctrl or Alt.';
    return;
  }
  stopRecording();
  const conflict = conflictFor(b, id);
  if (conflict) pending.value = { id, binding: b, conflict };
  else setBinding(id, b);
}

function requestReset(id: ActionId) {
  const conflict = resetConflict(id);
  if (conflict) pending.value = { id, binding: defaultBinding(id), conflict };
  else resetBinding(id);
}

function replace() {
  const p = pending.value;
  if (!p) return;
  setBinding(p.conflict, null);
  setBinding(p.id, p.binding);
  pending.value = null;
}

onUnmounted(stopRecording);
</script>

<template>
  <section>
    <div class="header">
      <h3>Keyboard shortcuts</h3>
      <button @click="resetAll()">Reset all</button>
    </div>
    <p v-if="pending" class="conflict">
      {{ formatBinding(pending.binding, IS_MAC) }} is used by {{ ACTIONS[pending.conflict].title }}.
      <button class="primary" @click="replace">Replace</button>
      <button @click="pending = null">Cancel</button>
    </p>
    <table class="shortcuts">
      <tbody>
        <tr v-for="id in ACTION_IDS" :key="id">
          <td>
            {{ ACTIONS[id].title }}
            <p v-if="inactiveInTerminal(id)" class="muted hint">
              Not active while a terminal is focused
            </p>
          </td>
          <td>
            <button
              class="binding"
              :class="{ recording: editing === id }"
              @click="startRecording(id)"
            >
              {{ editing === id ? 'Press keys…' : shortcutHint(id) || 'Not set' }}
            </button>
            <p v-if="editing === id && notice" class="muted hint">{{ notice }}</p>
          </td>
          <td class="row-actions">
            <button v-if="bindingFor(id)" @click="setBinding(id, null)">Clear</button>
            <button v-if="hasOverride(id)" @click="requestReset(id)">Reset</button>
          </td>
        </tr>
      </tbody>
    </table>
    <p class="muted">Click a shortcut and press the new keys. Esc cancels.</p>
  </section>
</template>

<style scoped>
.header {
  display: flex;
  align-items: center;
  justify-content: space-between;
}
.shortcuts {
  width: 100%;
  border-collapse: collapse;
}
.shortcuts td {
  padding: 6px 8px 6px 0;
  vertical-align: top;
}
.binding {
  min-width: 110px;
  font-variant-numeric: tabular-nums;
}
.binding.recording {
  outline: 2px solid currentColor;
}
.row-actions {
  text-align: right;
  white-space: nowrap;
}
.hint {
  margin: 2px 0 0;
  font-size: 11px;
}
.conflict button {
  margin-left: 6px;
}
</style>
