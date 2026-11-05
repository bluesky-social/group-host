<script lang="ts">
	import { enhance } from '$app/forms';
	import { inPlace } from '#lib/forms.ts';
	import Empty from '#lib/molecules/Empty.svelte';
	import MemberRow from '#lib/molecules/MemberRow.svelte';
	/** The invites the group has sent that are still waiting, each to cancel. */
	let { invites }: { invites: { uri: string; invitee: string; actor?: { did: string; handle?: string } }[] } = $props();
</script>

{#if invites.length}
	<ul class="invites">
		{#each invites as i (i.uri)}
			<li>
				<MemberRow actor={i.actor ?? { did: i.invitee }}>
					<form method="post" action="?/uninvite" use:enhance={inPlace}><input type="hidden" name="uri" value={i.uri} /><button class="quiet small" type="submit">Cancel invite</button></form>
				</MemberRow>
			</li>
		{/each}
	</ul>
{:else}
	<Empty title="No invites waiting">People you invite show up here until they join or decline.</Empty>
{/if}
