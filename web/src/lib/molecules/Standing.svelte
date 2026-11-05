<script lang="ts">
	import Tag from '#lib/atoms/Tag.svelte';
	/** A member's standing in the group: the roles they hold there, and the way to leave it. */
	let { roles }: { roles: string[] } = $props();
</script>

<form class="standing" method="post" action="?/leave">
	<span class="mute">You're a member, as</span>
	{#each roles as role (role)}<Tag text={role} />{/each}
	<button class="quiet" type="submit">Leave the group</button>
</form>
