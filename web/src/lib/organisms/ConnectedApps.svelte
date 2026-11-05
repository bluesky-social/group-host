<script lang="ts">
	import Empty from '#lib/molecules/Empty.svelte';
	/** The AI apps the person connected to the group, to act as them there: each to disconnect. */
	type App = { clientId: string; name: string; connectedAt: string; usedAt: string };
	let { apps }: { apps: App[] } = $props();
</script>

{#if apps.length}
	<ul class="ai-apps">
		{#each apps as a (a.clientId)}
			<li>
				<strong>{a.name}</strong>
				<span class="mute small">connected {a.connectedAt}, last used {a.usedAt}</span>
				<form method="post" action="?/disconnect"><input type="hidden" name="clientId" value={a.clientId} /><button class="quiet" type="submit">Disconnect {a.name}</button></form>
			</li>
		{/each}
	</ul>
{:else}
	<Empty title="No AI app connected yet">Add the address above to one, and it shows here.</Empty>
{/if}
