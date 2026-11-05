<script lang="ts">
	import ConnectAiApp from '#lib/organisms/ConnectAiApp.svelte';
	import ConnectedApps from '#lib/organisms/ConnectedApps.svelte';
	import GroupCard from '#lib/organisms/GroupCard.svelte';
	import InviteDialog from '#lib/organisms/InviteDialog.svelte';
	import Invites from '#lib/organisms/Invites.svelte';
	import JoinPanel from '#lib/organisms/JoinPanel.svelte';
	import JoinRequests from '#lib/organisms/JoinRequests.svelte';
	import MemberList from '#lib/organisms/MemberList.svelte';
	import Section from '#lib/molecules/Section.svelte';
	import Standing from '#lib/molecules/Standing.svelte';
	import SwitchedOff from '#lib/molecules/SwitchedOff.svelte';
	import { image, linksOf } from '#lib/images.ts';
	import GroupPage from '#lib/templates/GroupPage.svelte';
	import Welcome from '#lib/templates/Welcome.svelte';
	let { data, form } = $props();
	const g = $derived(data.group);
	const may = $derived(g.viewer?.actions ?? []);
	const member = $derived(g.viewer?.member ?? false);
	// What the page calls the group: its name, or its handle where its profile is not to be read.
	const name = $derived(g.displayName ?? g.handle);
	// The roles the person looking may give: those their roles' grants say.
	const assignable = $derived(
		may.includes('role.assign')
			? [...new Set((data.bindings ?? []).filter((b) => g.viewer?.roles.some((r) => r.role === b.role)).flatMap((b) => b.assignable.map((a) => a.role)))]
			: []
	);
	const joining: Record<string, string> = { approval: 'Joining is by request.', open: 'Anyone can join.', invite: 'Joining is by invite.' };
</script>

<svelte:head><title>{name} · opensocial</title></svelte:head>

{#if !data.viewer}
	<Welcome title={g.handle} handle>Sign in to see this group.</Welcome>
{:else}
	<GroupPage>
		{#snippet notice()}
			{#if form?.failed}<p class="danger">{form.failed}</p>{/if}
		{/snippet}
		{#snippet card()}
			<GroupCard handle={g.handle} name={g.displayName} description={g.description} avatar={image(g.did, g.avatar)} banner={image(g.did, g.banner)} location={g.location} online={g.online} languages={g.languages} topics={g.topics} links={linksOf(g.did, g.links)} rules={g.rules}>
				{#snippet nav()}
					{#if member && (data.settings || may.includes('mod.read'))}
						<nav class="group-nav" aria-label="The group">
							{#if may.includes('mod.read')}<a class="button quiet small" href={`/g/${g.handle}/moderation`}>Moderation</a>{/if}
							{#if data.settings}<a class="button quiet small" href={`/g/${g.handle}/settings`}>Settings</a>{/if}
						</nav>
					{/if}
				{/snippet}
				{#if member}
					<Standing roles={g.viewer?.roles.map((r) => r.role) ?? []} />
				{:else if g.displayName}
					<p class="mute small">{joining[g.joinPolicy ?? 'approval'] ?? joining.approval}</p>
				{/if}
			</GroupCard>
		{/snippet}
		{#if g.deactivated}
			<SwitchedOff steward={g.viewer?.steward ?? false} />
		{:else}
			{#if !member}
				<JoinPanel {name} joinPolicy={g.joinPolicy} asked={!!g.viewer?.request} invited={data.invited} />
			{:else if form?.joined}
				<JoinPanel {name} joined />
			{/if}
			{#if data.joinRequests}
				<Section title="Asking to join" count={data.joinRequests.length}>
					<JoinRequests requests={data.joinRequests} />
				</Section>
			{/if}
			{#if data.members}
				<Section title="Members" count={g.memberCount}>
					{#snippet actions()}
						{#if data.invites}<button class="quiet" type="button" popovertarget="invite">Invite someone</button>{/if}
					{/snippet}
					<MemberList members={data.members} q={data.q} shown={data.shown} more={data.more} me={data.viewer?.did} {assignable} canEject={may.includes('eject')} />
				</Section>
			{/if}
			{#if data.invites}
				<InviteDialog id="invite" />
				{#if data.invites.length}
					<Section title="Invites waiting" count={data.invites.length}>
						<Invites invites={data.invites} />
					</Section>
				{/if}
			{/if}
			{#if may.includes('group.configure')}
				<Section title="Connect an AI app">
					<ConnectAiApp {name} mcp={data.mcp} />
					<ConnectedApps apps={data.apps ?? []} />
				</Section>
			{:else if data.apps?.length}
				<Section title="Your AI apps">
					<ConnectedApps apps={data.apps} />
				</Section>
			{/if}
		{/if}
	</GroupPage>
{/if}
