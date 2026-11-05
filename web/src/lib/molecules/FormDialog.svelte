<script lang="ts">
	import type { Snippet } from 'svelte';
	import Dialog from '#lib/molecules/Dialog.svelte';
	/**
	 * An act that asks for input, in a dialog opened by a button whose
	 * `popovertarget` is `id`: `intro`, a line on what it does; the form,
	 * sent to `action`, its fields `children`; then Cancel, and `submit`,
	 * the act. `wide` for a form with much in it.
	 */
	let {
		id,
		title,
		action,
		submit,
		intro,
		multipart = false,
		wide = false,
		children
	}: { id: string; title: string; action: string; submit: string; intro?: Snippet; multipart?: boolean; wide?: boolean; children: Snippet } = $props();
</script>

<Dialog {id} {title} {wide}>
	{#if intro}<p class="mute">{@render intro()}</p>{/if}
	<form class="stack" method="post" {action} enctype={multipart ? 'multipart/form-data' : undefined}>
		{@render children()}
		<div class="actions">
			<button class="quiet" type="button" popovertarget={id} popovertargetaction="hide">Cancel</button>
			<button type="submit">{submit}</button>
		</div>
	</form>
</Dialog>
