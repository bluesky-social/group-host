<script lang="ts">
	import GroupProfile from '#lib/organisms/GroupProfile.svelte';
	import ConnectedApps from '#lib/organisms/ConnectedApps.svelte';
	import Invites from '#lib/organisms/Invites.svelte';
	import JoinRequests from '#lib/organisms/JoinRequests.svelte';
	import MemberList from '#lib/organisms/MemberList.svelte';
	import Standing from '#lib/molecules/Standing.svelte';
	import Page from '#lib/templates/Page.svelte';
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
	<p class="mute mono">@{g.handle}</p>
	{#if g.viewer?.member}
		<p>
			<a href={`/g/${g.handle}/settings`}>Settings</a>{#if g.viewer.actions?.includes('mod.read')} · <a href={`/g/${g.handle}/moderation`}>Moderation</a>{/if}
		</p>
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
			<h2>Members</h2>
			<MemberList members={data.members} me={data.viewer?.did} {assignable} canEject={actions.includes('eject')} />
		{/if}
		{#if data.invites}
			<h2>Invites</h2>
			<Invites invites={data.invites} />
		{/if}
		{#if data.joinRequests}
			<h2>Asking to join</h2>
			<JoinRequests requests={data.joinRequests} />
		{/if}
		{#if data.apps}
			<h2>Your assistants</h2>
			<ConnectedApps apps={data.apps} mcp={data.mcp} />
		{/if}
	{/if}
</Page>
