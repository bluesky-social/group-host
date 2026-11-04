<script lang="ts">
	import Empty from '#lib/molecules/Empty.svelte';
	/** The assistants the person connected to the group, to act as them there: each to disconnect. */
	type App = { clientId: string; name: string; connectedAt: string; usedAt: string };
	let { apps, mcp }: { apps: App[]; mcp: string } = $props();
</script>

<div class="stack">
	{#if apps.length}
		<ul class="assistants">
			{#each apps as a (a.clientId)}
				<li>
					<strong>{a.name}</strong> <span class="mute">connected {a.connectedAt}, last used {a.usedAt}</span>
					<form method="post" action="?/disconnect"><input type="hidden" name="clientId" value={a.clientId} /><button class="quiet" type="submit">Disconnect {a.name}</button></form>
				</li>
			{/each}
		</ul>
	{:else}
		<Empty title="No assistants connected">To have an assistant act as you in this group, add <code>{mcp}</code> to it as an MCP server.</Empty>
	{/if}
</div>
