<script lang="ts">
	import type { Snippet } from 'svelte';
	/**
	 * A part of a page, apart from the rest, as a card: its heading, with
	 * how many it holds (`count`) when that is worth saying, and the one
	 * act on the whole of it (`actions`), a quiet button, across from it;
	 * `intro`, a line on what it is; then what it holds, its rows or its
	 * empty state.
	 */
	let { title, count, actions, intro, children }: { title: string; count?: number; actions?: Snippet; intro?: Snippet; children: Snippet } = $props();
</script>

<section class="card stack">
	<div class="section-head">
		<h2>{title}{#if count !== undefined}<span class="count">{count}</span>{/if}</h2>
		{@render actions?.()}
		{#if intro}<p class="mute">{@render intro()}</p>{/if}
	</div>
	{@render children()}
</section>
