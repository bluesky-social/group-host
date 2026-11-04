<script lang="ts">
	import Empty from '#lib/molecules/Empty.svelte';
	import GroupRow from '#lib/molecules/GroupRow.svelte';
	import ScopeList from '#lib/molecules/ScopeList.svelte';
	type Group = { did: string; handle: string; displayName?: string; roles: { role: string }[]; granted: { scope: string }[]; withheld: { scope: string }[] };
	/**
	 * An app's request to sign in as a group: who it is, and for each group
	 * the person may sign it in as, what it would get and what it would not;
	 * approved for one, or declined.
	 */
	let {
		client,
		groups,
		labels,
		requestUri
	}: {
		client: { clientId: string; name?: string; uri?: string };
		groups: Group[];
		labels: { scope: string; text: string }[];
		requestUri: string;
	} = $props();
	// The request stays in the address the form is sent to, for the page
	// drawn after it.
	const action = (name: string) => `?request_uri=${encodeURIComponent(requestUri)}&/${name}`;
</script>

<div class="stack">
	<p><strong>{client.name ?? client.clientId}</strong> wants to sign in as one of your groups.</p>
	{#if client.uri}<p class="mute"><a href={client.uri}>{client.uri}</a></p>{/if}
	{#if groups.length}
		{#each groups as g (g.did)}
			<form class="stack" method="post" action={action('approve')}>
				<input type="hidden" name="requestUri" value={requestUri} />
				<input type="hidden" name="group" value={g.did} />
				<GroupRow name={g.displayName ?? g.handle} handle={g.handle} />
				{#if g.roles.length}
					<ScopeList title="It would be able to" scopes={g.granted} {labels} />
					<ScopeList title="It would not get" scopes={g.withheld} {labels} />
					<button type="submit">Sign it in as {g.displayName ?? g.handle}</button>
				{:else}
					<p class="mute">You hold no role in this group, so you cannot sign an app in as it.</p>
				{/if}
			</form>
		{/each}
	{:else}
		<Empty title="No group you can sign it in as">You hold no role in any group on this host.</Empty>
	{/if}
	<form method="post" action={action('decline')}>
		<input type="hidden" name="requestUri" value={requestUri} />
		<button type="submit">Decline</button>
	</form>
</div>
