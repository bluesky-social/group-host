<script lang="ts">
	import Empty from '#lib/molecules/Empty.svelte';
	/** The requests waiting to join, for a member who may admit them: each admitted or turned down. */
	let { requests }: { requests: { did: string; message?: string; actor?: { did: string; handle?: string } }[] } = $props();
</script>

{#if requests.length}
	<ul class="requests">
		{#each requests as r (r.did)}
			<li class="stack">
				<span class="mono">{r.actor?.handle ? `@${r.actor.handle}` : r.did}</span>
				{#if r.message}<p>{r.message}</p>{/if}
				<form method="post" action="?/admit"><input type="hidden" name="did" value={r.did} /><button type="submit">Admit</button></form>
				<form method="post" action="?/deny"><input type="hidden" name="did" value={r.did} /><button class="quiet" type="submit">Turn down</button></form>
			</li>
		{/each}
	</ul>
{:else}
	<Empty title="Nobody is waiting to join" />
{/if}
