<script lang="ts">
	import Empty from '#lib/molecules/Empty.svelte';
	/** Who is in the group, and their roles. */
	let { members }: { members: { actor: { did: string; handle?: string }; roles: { role: string }[] }[] } = $props();
</script>

{#if members.length}
	<ul class="members">
		{#each members as m (m.actor.did)}
			<li><span class="mono">{m.actor.handle ? `@${m.actor.handle}` : m.actor.did}</span> <span class="mute">{m.roles.map((r) => r.role).join(', ')}</span></li>
		{/each}
	</ul>
{:else}
	<Empty title="No members yet" />
{/if}
