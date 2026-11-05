<script lang="ts">
	import Tabs from '#lib/molecules/Tabs.svelte';
	import Page from '#lib/templates/Page.svelte';
	import { page } from '$app/state';
	let { data, children } = $props();
	const g = $derived(data.group);
	const at = $derived(`/g/${g.handle}/settings`);
	// The parts the person looking may change, each a page.
	const tabs = $derived([
		...(data.configures
			? [
					{ href: at, text: 'Profile' },
					{ href: `${at}/rules`, text: 'Rules' },
					{ href: `${at}/roles`, text: 'Roles' }
				]
			: []),
		...(data.configures || data.passwords ? [{ href: `${at}/apps`, text: 'Apps' }] : []),
		...(data.configures ? [{ href: `${at}/recovery`, text: 'Recovery keys' }] : [])
	]);
</script>

<Page title="Settings" back={{ href: `/g/${g.handle}`, text: g.displayName ?? g.handle }}>
	{#snippet nav()}
		{#if tabs.length > 1}<Tabs {tabs} current={page.url.pathname} label="Settings" />{/if}
	{/snippet}
	{@render children()}
</Page>
