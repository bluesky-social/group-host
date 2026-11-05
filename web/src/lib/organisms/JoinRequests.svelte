<script lang="ts">
	import { enhance } from '$app/forms';
	import { inPlace } from '#lib/forms.ts';
	import Empty from '#lib/molecules/Empty.svelte';
	import MemberRow from '#lib/molecules/MemberRow.svelte';
	/** The requests waiting to join, for a member whose roles admit people: each with its note, approved or declined. */
	let { requests }: { requests: { member: string; message?: string; actor?: { did: string; handle?: string } }[] } = $props();
</script>

{#if requests.length}
	<ul class="requests">
		{#each requests as r (r.member)}
			<li>
				<MemberRow actor={r.actor ?? { did: r.member }} note={r.message}>
					<form method="post" action="?/deny" use:enhance={inPlace}><input type="hidden" name="did" value={r.member} /><button class="quiet small" type="submit">Decline</button></form>
					<form method="post" action="?/admit" use:enhance={inPlace}><input type="hidden" name="did" value={r.member} /><button class="small" type="submit">Approve</button></form>
				</MemberRow>
			</li>
		{/each}
	</ul>
{:else}
	<Empty title="Nobody is asking to join">Requests to join show up here.</Empty>
{/if}
