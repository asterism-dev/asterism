import { reactive, watch } from 'vue';
import { SIDEBAR_KEY, parseSidebar } from './model';
import { read, write } from './storage';

export const sidebar = reactive(parseSidebar(read(SIDEBAR_KEY)));

watch(sidebar, (value) => write(SIDEBAR_KEY, JSON.stringify(value)));
