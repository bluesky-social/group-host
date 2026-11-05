<script lang="ts">
	import Empty from '#lib/molecules/Empty.svelte';
	import MemberRow from '#lib/molecules/MemberRow.svelte';
	/** The invites the group has sent that are still waiting, each to take back. */
	let { invites }: { invites: { uri: string; invitee: string; actor?: { did: string; handle?: string } }[] } = $props();
</script>

{#if invites.length}
	<ul class="invites">
		{#each invites as i (i.uri)}
			<li>
				<MemberRow actor={i.actor ?? { did: i.invitee }}>
					<form method="post" action="?/uninvite"><input type="hidden" name="uri" value={i.uri} /><button class="quiet small" type="submit">Take back</button></form>
				</MemberRow>
			</li>
		{/each}
	</ul>
{:else}
	<Empty title="No invites waiting">Whoever you invite shows here until they join or turn it down.</Empty>
{/if}
