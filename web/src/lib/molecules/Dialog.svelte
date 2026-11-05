<script lang="ts">
	import type { Snippet } from 'svelte';
	import { empty } from '#lib/forms.ts';
	/**
	 * A dialog over the dimmed page, opened by a button whose
	 * `popovertarget` is its `id`: its `title`, a
	 * button to close it, then `children`. Light dismiss closes it too, as a
	 * popover does, with no script. `wide` for one with much in it. Closed,
	 * however it closes, what was typed into it goes, and `onclose` is told.
	 */
	let { id, title, wide = false, onclose, children }: { id: string; title: string; wide?: boolean; onclose?: () => void; children: Snippet } = $props();
	// Opened, the first thing to fill in is the one being typed into.
	function toggled(e: ToggleEvent) {
		if (e.newState !== 'open') {
			(e.currentTarget as HTMLElement).querySelectorAll('form').forEach(empty);
			onclose?.();
			return;
		}
		const into = (e.currentTarget as HTMLElement).querySelector<HTMLInputElement>('input:not([type=hidden]),textarea');
		into?.focus();
	}
</script>

<div class="dialog" class:dialog-wide={wide} {id} popover="auto" role="dialog" aria-modal="true" aria-labelledby="{id}-title" ontoggle={toggled}>
	<h2 id="{id}-title" class="dialog-title">{title}</h2>
	<button class="close" type="button" popovertarget={id} popovertargetaction="hide" aria-label="Close">×</button>
	{@render children()}
</div>
