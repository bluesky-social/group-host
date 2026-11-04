<script lang="ts">
	/**
	 * Where the person looking stands with the group, and what they can do
	 * about it: join or ask to, take back their request, or leave.
	 */
	let {
		member,
		asked,
		joinPolicy
	}: { member: boolean; asked: boolean; joinPolicy?: string } = $props();
	const invited = $derived(joinPolicy?.endsWith('invite'));
	const open = $derived(joinPolicy?.endsWith('open'));
</script>

<div class="standing">
	{#if member}
		<form method="post" action="?/leave"><span class="mute">You are a member.</span> <button type="submit">Leave</button></form>
	{:else if asked}
		<form method="post" action="?/cancel"><span class="mute">You have asked to join.</span> <button type="submit">Take back the request</button></form>
	{:else if invited}
		<p class="mute">This group takes people by invitation.</p>
	{:else}
		<form method="post" action="?/join"><button type="submit">{open ? 'Join' : 'Ask to join'}</button></form>
	{/if}
</div>
