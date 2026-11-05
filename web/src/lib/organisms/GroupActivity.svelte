<script lang="ts">
	import Empty from '#lib/molecules/Empty.svelte';
	import { when } from '#lib/dates.ts';
	/** What the apps acting as the group did lately, newest first: when, and what. */
	let { activity }: { activity: { member: string; clientId?: string; action: string; at: string }[] } = $props();
	// A page of them at a time, more as asked for.
	let shown = $state(10);
</script>

{#if activity.length}
	<ul class="activity">
		{#each activity.slice(0, shown) as a, i (i)}
			<li><span class="row-main"><span class="what">{when(a.at)}</span><span class="mono small">{a.action}</span></span></li>
		{/each}
	</ul>
	{#if activity.length > shown}<button class="quiet small" type="button" onclick={() => (shown += 10)}>Show more</button>{/if}
{:else}
	<Empty title="Nothing done as the group yet">What apps write as the group shows here.</Empty>
{/if}
