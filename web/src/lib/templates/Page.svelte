<script lang="ts">
	import type { Snippet } from 'svelte';
	/** A page of the console: its heading, then what it holds. */
	let { title, children }: { title: string; children: Snippet } = $props();
</script>

<div class="stack">
	<h1>{title}</h1>
	{@render children()}
</div>
