<script lang="ts">
	import Empty from '#lib/molecules/Empty.svelte';
	import MemberRow from '#lib/molecules/MemberRow.svelte';
	/** The requests waiting to join, for a member who may let people in: each with its note, let in or turned down. */
	let { requests }: { requests: { member: string; message?: string; actor?: { did: string; handle?: string } }[] } = $props();
</script>

{#if requests.length}
	<ul class="requests">
		{#each requests as r (r.member)}
			<li>
				<MemberRow actor={r.actor ?? { did: r.member }} />
				<span class="row-actions">
					<form method="post" action="?/deny"><input type="hidden" name="did" value={r.member} /><button class="quiet" type="submit">Turn down</button></form>
					<form method="post" action="?/admit"><input type="hidden" name="did" value={r.member} /><button type="submit">Let in</button></form>
				</span>
				{#if r.message}<p class="mute">{r.message}</p>{/if}
			</li>
		{/each}
	</ul>
{:else}
	<Empty title="Nobody is asking to join">Whoever asks shows here, with their note.</Empty>
{/if}
