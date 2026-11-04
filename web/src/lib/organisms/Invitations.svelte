<script lang="ts">
	/** The groups that have invited the person looking: each to join, or turn down. */
	let { invitations }: { invitations: { group: { did: string; handle: string; displayName?: string }; message?: string }[] } = $props();
</script>

<ul class="invitations">
	{#each invitations as i (i.group.did)}
		<li class="stack">
			<span><strong>{i.group.displayName ?? i.group.handle}</strong> invites you to join.</span>
			{#if i.message}<p>{i.message}</p>{/if}
			<form method="post" action="?/accept"><input type="hidden" name="group" value={i.group.did} /><button type="submit">Join {i.group.displayName ?? i.group.handle}</button></form>
			<form method="post" action="?/decline"><input type="hidden" name="group" value={i.group.did} /><button class="quiet" type="submit">Turn it down</button></form>
		</li>
	{/each}
</ul>
