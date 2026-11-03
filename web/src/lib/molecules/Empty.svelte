<script lang="ts">
	import type { Snippet } from 'svelte';
	/**
	 * What a list or page says where its rows would be, when it has none:
	 * `title`, what is empty, in the reader's terms; `children`, a line on
	 * why, or the way out.
	 */
	let { title, children }: { title: string; children?: Snippet } = $props();
</script>

<div class="empty">
	<p class="empty-title">{title}</p>
	{#if children}<p>{@render children()}</p>{/if}
</div>
