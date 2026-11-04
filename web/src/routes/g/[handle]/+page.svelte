<script lang="ts">
	import GroupProfile from '#lib/organisms/GroupProfile.svelte';
	import ConnectedApps from '#lib/organisms/ConnectedApps.svelte';
	import Invites from '#lib/organisms/Invites.svelte';
	import JoinRequests from '#lib/organisms/JoinRequests.svelte';
	import MemberList from '#lib/organisms/MemberList.svelte';
	import Standing from '#lib/molecules/Standing.svelte';
	import Avatar from '#lib/atoms/Avatar.svelte';
	import { image } from '#lib/images.ts';
	import Page from '#lib/templates/Page.svelte';
	import Section from '#lib/molecules/Section.svelte';
	let { data, form } = $props();
	const g = $derived(data.group);
	const actions = $derived(g.viewer?.actions ?? []);
	// The roles the person looking may give: those their roles' grants say.
	const assignable = $derived(
		actions.includes('role.assign')
			? [...new Set((data.bindings ?? []).filter((b) => g.viewer?.roles.some((r) => r.role === b.role)).flatMap((b) => b.assignable.map((a) => a.role)))]
			: []
	);
</script>

<svelte:head><title>{g.displayName ?? g.handle} · opensocial</title></svelte:head>

<Page title={g.displayName ?? g.handle}>
	{#if g.banner}<img class="banner" src={image(g.did, g.banner)} alt="" />{/if}
	<p class="identity"><Avatar src={image(g.did, g.avatar)} size="md" /></p>
	<p class="mute mono">@{g.handle}</p>
	{#if g.viewer?.member}
		<nav class="links">
			<a href={`/g/${g.handle}/settings`}>Settings</a>
			{#if g.viewer.actions?.includes('mod.read')}<a href={`/g/${g.handle}/moderation`}>Moderation</a>{/if}
		</nav>
	{/if}
	{#if form?.failed}<p class="danger">{form.failed}</p>{/if}
	{#if g.deactivated}
		<p>This group is switched off.</p>
	{:else}
		<GroupProfile description={g.description} rules={g.rules} />
		{#if g.viewer}
			<Standing member={g.viewer.member} asked={!!g.viewer.request} joinPolicy={g.joinPolicy} />
		{/if}
		{#if data.members}
			<Section title="Members">
				<MemberList members={data.members} me={data.viewer?.did} {assignable} canEject={actions.includes('eject')} />
			</Section>
		{/if}
		{#if data.invites}
			<Section title="Invites">
				<Invites invites={data.invites} />
			</Section>
		{/if}
		{#if data.joinRequests}
			<Section title="Asking to join">
				<JoinRequests requests={data.joinRequests} />
			</Section>
		{/if}
		{#if data.apps}
			<Section title="Your assistants">
				<ConnectedApps apps={data.apps} mcp={data.mcp} />
			</Section>
		{/if}
	{/if}
</Page>
