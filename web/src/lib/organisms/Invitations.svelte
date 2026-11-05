<script lang="ts">
	import { enhance } from '$app/forms';
	import { inPlace } from '#lib/forms.ts';
	import Empty from '#lib/molecules/Empty.svelte';
	import GroupRow from '#lib/molecules/GroupRow.svelte';
	import { image } from '#lib/images.ts';
	/** The groups that have invited the person looking, a row each with its note: each to join, or decline; or that none is waiting. */
	let {
		invitations
	}: { invitations: { group: { did: string; handle: string; displayName?: string; avatar?: string }; message?: string }[] } = $props();
</script>

{#if !invitations.length}
	<Empty title="No invites waiting">Invites from groups show up here.</Empty>
{:else}
	<ul class="invitations">
		{#each invitations as i (i.group.did)}
			<li>
				<GroupRow name={i.group.displayName ?? i.group.handle} handle={i.group.handle} avatar={image(i.group.did, i.group.avatar)} note={i.message} />
				<span class="row-actions">
					<form method="post" action="?/decline" use:enhance={inPlace}><input type="hidden" name="group" value={i.group.did} /><button class="quiet small" type="submit">Decline</button></form>
					<form method="post" action="?/accept" use:enhance={inPlace}>
						<input type="hidden" name="group" value={i.group.did} /><button class="small" type="submit" aria-label={`Join ${i.group.displayName ?? i.group.handle}`}>Join</button>
					</form>
				</span>
			</li>
		{/each}
	</ul>
{/if}
