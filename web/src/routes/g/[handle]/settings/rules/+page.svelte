<script lang="ts">
	import Section from '#lib/molecules/Section.svelte';
	import RulesEditor from '#lib/organisms/RulesEditor.svelte';
	let { data, form } = $props();
	const g = $derived(data.group);
</script>

<svelte:head><title>Rules · {g.displayName ?? g.handle} · opensocial</title></svelte:head>

{#if form?.failed}<p class="danger">{form.failed}</p>{/if}
<Section title="Rules" count={g.rules?.length ?? 0}>
	{#snippet actions()}<button class="quiet" type="button" popovertarget="add-rule">Add a rule</button>{/snippet}
	{#snippet intro()}Rules everyone in the group agrees to. They show on the group's page in this order.{/snippet}
	<RulesEditor rules={g.rules} />
</Section>
