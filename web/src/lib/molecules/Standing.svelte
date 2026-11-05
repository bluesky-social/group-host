<script lang="ts">
	import ConfirmDialog from '#lib/molecules/ConfirmDialog.svelte';
	/**
	 * A member's standing in the group: the roles they hold there
	 * beyond being a member, which everyone is, in a sentence; and the way
	 * to leave it, asked to be sure of.
	 */
	let { roles }: { roles: string[] } = $props();
	const held = $derived(roles.filter((r) => r !== 'member'));
	const as = $derived(held.length ? ` with the ${held.length > 1 ? `${held.slice(0, -1).join(', ')} and ${held.at(-1)} roles` : `${held[0]} role`}` : '');
</script>

<div class="standing">
	<span class="mute">You're a member{as}.</span>
	<button class="quiet small" type="button" popovertarget="leave">Leave the group</button>
</div>
<ConfirmDialog id="leave" title="Leave this group?" action="?/leave" confirm="Leave the group">
	You'll lose access to what only members see. You can ask to join again later.
</ConfirmDialog>
