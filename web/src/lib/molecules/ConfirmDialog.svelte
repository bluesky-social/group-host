<script lang="ts">
	import type { Snippet } from 'svelte';
	import Dialog from '#lib/molecules/Dialog.svelte';
	/**
	 * Asking to be sure of what cannot be undone, opened by a button whose
	 * `popovertarget` is `id`: `children`, what will happen, in a sentence;
	 * Cancel; and `confirm`, the act, sent to `action` with `fields`.
	 */
	let {
		id,
		title,
		action,
		confirm,
		fields = {},
		children
	}: { id: string; title: string; action: string; confirm: string; fields?: Record<string, string>; children: Snippet } = $props();
</script>

<Dialog {id} {title}>
	<p class="mute">{@render children()}</p>
	<form method="post" {action} class="actions">
		{#each Object.entries(fields) as [name, value] (name)}<input type="hidden" {name} {value} />{/each}
		<button class="quiet" type="button" popovertarget={id} popovertargetaction="hide">Cancel</button>
		<button class="danger" type="submit">{confirm}</button>
	</form>
</Dialog>
