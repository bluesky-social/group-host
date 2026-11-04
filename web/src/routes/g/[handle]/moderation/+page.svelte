<script lang="ts">
	import LabelForm from '#lib/organisms/LabelForm.svelte';
	import ModerationQueue from '#lib/organisms/ModerationQueue.svelte';
	import Page from '#lib/templates/Page.svelte';
	import Section from '#lib/molecules/Section.svelte';
	let { data, form } = $props();
	const g = $derived(data.group);
</script>

<svelte:head><title>Moderation · {g.displayName ?? g.handle} · opensocial</title></svelte:head>

<Page title={`${g.displayName ?? g.handle}: moderation`}>
	<p><a href={`/g/${g.handle}`}>Back to the group</a></p>
	{#if form?.failed}<p class="danger">{form.failed}</p>{/if}
	{#if data.subjects}
		<p>
			<a href="?">Everything</a> · <a href="?status=open">Open</a> · <a href="?status=escalated">Escalated</a> ·
			<a href="?status=resolved">Resolved</a>
		</p>
		<ModerationQueue subjects={data.subjects} canResolve={data.actions.includes('mod.resolve')} />
	{:else}
		<p>Nothing here is yours to see.</p>
	{/if}
	{#if data.actions.includes('label')}
		<Section title="Label someone">
			<LabelForm />
		</Section>
	{/if}
</Page>
