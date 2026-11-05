<script lang="ts">
	import { enhance } from '$app/forms';
	import DeleteGroup from '#lib/molecules/DeleteGroup.svelte';
	import Empty from '#lib/molecules/Empty.svelte';
	import { inPlace } from '#lib/forms.ts';
	/**
	 * A group switched off, in the place of all it shows: to one
	 * of those who ran it then (`steward`), the way to switch it back on,
	 * and to delete it; to anyone else, the way back to their groups.
	 */
	let { steward }: { steward: boolean } = $props();
</script>

<Empty title="This group is switched off">
	An admin can switch it back on.
	{#snippet action()}
		{#if steward}
			<form method="post" action="?/activate" class="actions start" use:enhance={inPlace}>
				<button class="quiet" type="button" popovertarget="delete-group">Delete the group</button>
				<button type="submit">Switch back on</button>
			</form>
		{:else}
			<a class="button quiet" href="/">Back to your groups</a>
		{/if}
	{/snippet}
</Empty>
{#if steward}<DeleteGroup />{/if}
