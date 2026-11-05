<script lang="ts">
	import '../app.css';
	import SiteHeader from '#lib/organisms/SiteHeader.svelte';
	import SiteFooter from '#lib/organisms/SiteFooter.svelte';
	import { page } from '$app/state';
	let { data, children } = $props();
</script>

<SiteHeader viewer={data.viewer} />
<main class="wrap">{@render children()}</main>
<SiteFooter host={page.url.hostname} />
