<script lang="ts">
	/** A group as its profile says: its words, and its rules in order. */
	let {
		description,
		rules = []
	}: { description?: string; rules?: { rkey: string; title: string; text?: string }[] } = $props();
</script>

<div class="stack">
	{#if description}<p>{description}</p>{/if}
	{#if rules.length}
		<h2>Rules</h2>
		<ol>
			{#each rules as r (r.rkey)}
				<li><strong>{r.title}</strong>{#if r.text}<p class="mute">{r.text}</p>{/if}</li>
			{/each}
		</ol>
	{/if}
</div>
