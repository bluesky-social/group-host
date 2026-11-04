<script lang="ts">
	import AppPasswords from '#lib/organisms/AppPasswords.svelte';
	import GroupApps from '#lib/organisms/GroupApps.svelte';
	import ProfileForm from '#lib/organisms/ProfileForm.svelte';
	import RolesEditor from '#lib/organisms/RolesEditor.svelte';
	import RulesEditor from '#lib/organisms/RulesEditor.svelte';
	import Page from '#lib/templates/Page.svelte';
	import { ACTIONS } from '#lib/actions.ts';
	let { data, form } = $props();
	const g = $derived(data.group);
</script>

<svelte:head><title>Settings · {g.displayName ?? g.handle} · opensocial</title></svelte:head>

<Page title={`${g.displayName ?? g.handle}: settings`}>
	<p><a href={`/g/${g.handle}`}>Back to the group</a></p>
	{#if form?.failed}<p class="danger">{form.failed}</p>{/if}
	{#if data.configures}
		<h2>Profile</h2>
		<ProfileForm displayName={g.displayName} description={g.description} joinPolicy={g.joinPolicy} url={g.url} />
		<h2>Rules</h2>
		<RulesEditor rules={g.rules} />
		<h2>Roles</h2>
		<RolesEditor roles={data.roles} bindings={data.bindings} actions={ACTIONS} />
		<h2>Apps acting as the group</h2>
		<GroupApps sessions={data.tokens?.sessions ?? []} activity={data.tokens?.activity ?? []} />
	{/if}
	{#if data.passwords}
		<h2>App passwords</h2>
		<AppPasswords passwords={data.passwords} made={form?.made} />
	{/if}
	{#if !data.configures && !data.passwords}
		<p>Nothing here is yours to change.</p>
	{/if}
</Page>
