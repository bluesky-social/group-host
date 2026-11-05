<script lang="ts">
	import type { Snippet } from 'svelte';
	/**
	 * A part of a page, apart from the rest: its heading, with how many it
	 * holds (`count`) when that is worth saying, and what may be done to
	 * the whole of it (`actions`) across from it; then what it holds.
	 */
	let { title, count, actions, children }: { title: string; count?: number; actions?: Snippet; children: Snippet } = $props();
</script>

<section class="card stack">
	<div class="section-head">
		<h2>{title}{#if count !== undefined} <span class="count">{count}</span>{/if}</h2>
		{@render actions?.()}
	</div>
	{@render children()}
</section>
