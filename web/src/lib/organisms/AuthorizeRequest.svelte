<script lang="ts">
	import Empty from '#lib/molecules/Empty.svelte';
	import GroupRow from '#lib/molecules/GroupRow.svelte';
	import ScopeList from '#lib/molecules/ScopeList.svelte';
	type Group = { did: string; handle: string; displayName?: string; roles: { role: string }[]; granted: { scope: string }[]; withheld: { scope: string }[] };
	/**
	 * An app's request to sign in as a group: who it is, and for each group
	 * the person may sign it in as, what it would get and what it would not;
	 * approved for one, or declined. An AI app's, an MCP client's (`mcp`),
	 * asks to act as the person in a group instead, with what their roles
	 * there allow.
	 */
	let {
		client,
		groups,
		labels,
		requestUri,
		mcp = false
	}: {
		client: { clientId: string; name?: string; uri?: string };
		groups: Group[];
		labels: { scope: string; text: string }[];
		requestUri: string;
		mcp?: boolean;
	} = $props();
	const name = $derived(client.name ?? client.clientId);
	// The request stays in the address the form is sent to, for the page
	// drawn after it.
	const action = (name: string) => `?request_uri=${encodeURIComponent(requestUri)}&/${name}`;
</script>

<div class="stack">
	{#if mcp}
		<p>An AI app calling itself <strong>{name}</strong> wants to act as you in one of your groups.</p>
		<p class="mute">That name is its own word for itself, not checked. It came from <span class="mono">{client.clientId}</span>.</p>
	{:else}
		<p><strong>{name}</strong> wants to sign in as one of your groups.</p>
	{/if}
	{#if client.uri}<p class="mute"><a href={client.uri}>{client.uri}</a></p>{/if}
	{#if groups.length}
		{#each groups as g (g.did)}
			<form class="stack" method="post" action={action('approve')}>
				<input type="hidden" name="requestUri" value={requestUri} />
				<input type="hidden" name="group" value={g.did} />
				<GroupRow name={g.displayName ?? g.handle} handle={g.handle} />
				{#if g.roles.length && mcp}
					<p>It could do what your roles there let you: {g.roles.map((r) => r.role).join(', ')}.</p>
					<button type="submit">Connect it to {g.displayName ?? g.handle}</button>
				{:else if g.roles.length}
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
		<button class="quiet" type="submit">Decline</button>
	</form>
</div>
