<script lang="ts">
	/** The invites the group has sent that are still waiting, each to take back. */
	let { invites }: { invites: { uri: string; invitee: string }[] } = $props();
</script>

<ul class="invites">
	{#each invites as i (i.uri)}
		<li>
			<span class="mono">{i.invitee}</span>
			<form method="post" action="?/uninvite"><input type="hidden" name="uri" value={i.uri} /><button class="quiet" type="submit">Take back</button></form>
		</li>
	{/each}
</ul>
