import { ref } from 'vue';
import { formatRoute, parseRoute, type Route } from './route';

export const route = ref<Route>(parseRoute(location.hash));

window.addEventListener('hashchange', () => (route.value = parseRoute(location.hash)));

export function go(next: Route) {
  location.hash = formatRoute(next);
}
