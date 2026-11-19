import { mount } from 'svelte';
import App from './App.svelte';
// Archivo, across its widths and weights, bundled with the site (DESIGN.md).
import '@fontsource-variable/archivo/wdth.css';
import './app.css';

mount(App, { target: document.getElementById('app') });
