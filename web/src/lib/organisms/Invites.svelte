<script lang="ts">
	import Empty from '#lib/molecules/Empty.svelte';
	/** Inviting someone, by their handle, and the invites waiting, each to take back. */
	let { invites }: { invites: { uri: string; invitee: string }[] } = $props();
</script>

<div class="stack">
	<form method="post" action="?/invite">
		<label>Their handle <input name="handle" required placeholder="someone.bsky.social" /></label>
		<button type="submit">Invite</button>
	</form>
	{#if invites.length}
		<ul class="invites">
			{#each invites as i (i.uri)}
				<li>
					<span class="mono">{i.invitee}</span>
					<form method="post" action="?/uninvite"><input type="hidden" name="uri" value={i.uri} /><button class="quiet" type="submit">Take back</button></form>
				</li>
			{/each}
		</ul>
	{:else}
		<Empty title="No invites waiting" />
	{/if}
</div>
