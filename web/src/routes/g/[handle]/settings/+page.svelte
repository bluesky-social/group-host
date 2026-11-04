<script lang="ts">
	import AppPasswords from '#lib/organisms/AppPasswords.svelte';
	import GroupApps from '#lib/organisms/GroupApps.svelte';
	import ProfileForm from '#lib/organisms/ProfileForm.svelte';
	import ProfilePictures from '#lib/organisms/ProfilePictures.svelte';
	import Visibility from '#lib/organisms/Visibility.svelte';
	import { image } from '#lib/images.ts';
	import RecoveryKeys from '#lib/organisms/RecoveryKeys.svelte';
	import RolesEditor from '#lib/organisms/RolesEditor.svelte';
	import RulesEditor from '#lib/organisms/RulesEditor.svelte';
	import Page from '#lib/templates/Page.svelte';
	import Section from '#lib/molecules/Section.svelte';
	import { ACTIONS } from '#lib/actions.ts';
	let { data, form } = $props();
	const g = $derived(data.group);
</script>

<svelte:head><title>Settings · {g.displayName ?? g.handle} · opensocial</title></svelte:head>

<Page title={`${g.displayName ?? g.handle}: settings`}>
	<p><a href={`/g/${g.handle}`}>Back to the group</a></p>
	{#if form?.failed}<p class="danger">{form.failed}</p>{/if}
	{#if data.configures}
		<Section title="Profile">
			<ProfileForm displayName={g.displayName} description={g.description} joinPolicy={g.joinPolicy} url={g.url} />
		</Section>
		<Section title="Pictures">
			<ProfilePictures avatar={image(g.did, g.avatar)} banner={image(g.did, g.banner)} />
		</Section>
		<Section title="Who sees it">
			<Visibility listed={data.listed} />
		</Section>
		<Section title="Rules">
			<RulesEditor rules={g.rules} />
		</Section>
		<Section title="Roles">
			<RolesEditor roles={data.roles} bindings={data.bindings} actions={ACTIONS} />
		</Section>
		{#if data.recovery}
			<Section title="Recovery keys">
				<RecoveryKeys keys={data.recovery.keys} hostHolds={data.recovery.hostHolds} max={data.recovery.max} />
			</Section>
		{/if}
		<Section title="Apps acting as the group">
			<GroupApps sessions={data.tokens?.sessions ?? []} activity={data.tokens?.activity ?? []} />
		</Section>
	{/if}
	{#if data.passwords}
		<Section title="App passwords">
			<AppPasswords passwords={data.passwords} made={form?.made} />
		</Section>
	{/if}
	{#if !data.configures && !data.passwords}
		<p>Nothing here is yours to change.</p>
	{/if}
</Page>
