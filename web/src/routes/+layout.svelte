<script lang="ts">
	import '../app.css';
	import SiteHeader from '#lib/organisms/SiteHeader.svelte';
	import SiteFooter from '#lib/organisms/SiteFooter.svelte';
	let { data, children } = $props();
</script>

<SiteHeader viewer={data.viewer} />
<main class="wrap">{@render children()}</main>
<SiteFooter />
