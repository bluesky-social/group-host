<script lang="ts">
	import type { Snippet } from 'svelte';
	/** A part of a page, apart from the rest: its heading, then what it holds. */
	let { title, children }: { title: string; children: Snippet } = $props();
</script>

<section class="card stack">
	<h2>{title}</h2>
	{@render children()}
</section>
