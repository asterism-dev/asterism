import { createApp } from 'vue';
import App from './App.vue';
import './styles.css';
import { initTheme } from './theme';

// Only text fields keep the native menu (copy/paste); elsewhere WebKit would offer Reload.
window.addEventListener('contextmenu', (e) => {
  const t = e.target;
  const textField = (t instanceof HTMLInputElement || t instanceof HTMLTextAreaElement) && !t.classList.contains('xterm-helper-textarea');
  if (!textField) e.preventDefault();
});

initTheme();
createApp(App).mount('#app');
