<script lang="ts">
	import Section from '#lib/molecules/Section.svelte';
	import RolesEditor from '#lib/organisms/RolesEditor.svelte';
	import { ACTIONS } from '#lib/actions.ts';
	let { data, form } = $props();
	const g = $derived(data.group);
</script>

<svelte:head><title>Roles · {g.displayName ?? g.handle} · opensocial</title></svelte:head>

{#if form?.failed}<p class="danger">{form.failed}</p>{/if}
<Section title="Roles" count={data.roles?.length ?? 0}>
	{#snippet actions()}<button class="quiet" type="button" popovertarget="define-role">Create a role</button>{/snippet}
	{#snippet intro()}What people with each role can do. You give roles to members from the group's page.{/snippet}
	<RolesEditor roles={data.roles} bindings={data.bindings} actions={ACTIONS} />
</Section>
