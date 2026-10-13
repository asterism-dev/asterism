import { createApp } from 'vue';
import '../styles.css';
import './mobile.css';
import { initTheme } from '../theme';
import { connect } from './connection';
import MobileApp from './MobileApp.vue';
import { adoptPairing } from './pairing';

initTheme();
connect(adoptPairing(location, history, localStorage));
createApp(MobileApp).mount('#app');
