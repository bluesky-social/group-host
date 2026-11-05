<script lang="ts">
	import Empty from '#lib/molecules/Empty.svelte';
	import Section from '#lib/molecules/Section.svelte';
	import Tabs from '#lib/molecules/Tabs.svelte';
	import LabelDialog from '#lib/organisms/LabelDialog.svelte';
	import ModerationQueue from '#lib/organisms/ModerationQueue.svelte';
	import Page from '#lib/templates/Page.svelte';
	import { page } from '$app/state';
	let { data, form } = $props();
	const g = $derived(data.group);
	const at = $derived(`/g/${g.handle}/moderation`);
	const tabs = $derived(['open', 'escalated', 'resolved'].map((s) => ({ href: `${at}?status=${s}`, text: s[0].toUpperCase() + s.slice(1) })));
	const label = $derived(data.actions.includes('label'));
</script>

<svelte:head><title>Moderation · {g.displayName ?? g.handle} · opensocial</title></svelte:head>

<Page title="Moderation" back={{ href: `/g/${g.handle}`, text: g.displayName ?? g.handle }}>
	{#snippet actions()}
		{#if label}<button class="quiet" type="button" popovertarget="label">Label someone</button>{/if}
	{/snippet}
	{#snippet nav()}
		{#if data.subjects}<Tabs {tabs} current={`${page.url.pathname}?status=${data.status}`} label="The queue" />{/if}
	{/snippet}
	{#if form?.failed}<p class="danger">{form.failed}</p>{/if}
	{#if data.subjects}
		<Section title="Subjects" count={data.subjects.length}>
			<ModerationQueue subjects={data.subjects} status={data.status} canResolve={data.actions.includes('mod.resolve')} />
		</Section>
	{:else}
		<Empty title="Nothing here is yours to see">Those who may read the group's moderation queue see it here.</Empty>
	{/if}
	{#if label}<LabelDialog id="label" />{/if}
</Page>
