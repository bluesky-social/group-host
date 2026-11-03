<script lang="ts">
	import Empty from '#lib/molecules/Empty.svelte';
	import GroupRow from '#lib/molecules/GroupRow.svelte';
	/** The groups a host runs, or what it says when it runs none. */
	let { groups }: { groups: { did: string; handle: string; displayName?: string }[] } = $props();
</script>

{#if groups.length}
	<div>{#each groups as g (g.did)}<GroupRow name={g.displayName ?? g.handle} handle={g.handle} />{/each}</div>
{:else}
	<Empty title="No groups here yet">A group started on this host shows here once its profile is public.</Empty>
{/if}
