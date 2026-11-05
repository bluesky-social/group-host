<script lang="ts">
	/**
	 * A round picture of a group or a person; where there is none, the
	 * first letter of `name` on a hue of its own, the same for the same
	 * name wherever it is drawn.
	 */
	let { src, size, name = '' }: { src?: string | null; size?: 'md' | 'lg'; name?: string } = $props();
	let cls = $derived(size ? `av ${size}` : 'av');
	// Its first letter, past the @ and the did: a handle or DID starts with;
	// drawn by the stylesheet, so it is never read as part of a name beside it.
	const plain = $derived(name.replace(/^@/, '').replace(/^did:[a-z]+:/, ''));
	const initial = $derived([...plain][0]?.toUpperCase() ?? '');
	const hue = $derived([...name].reduce((h, c) => (h * 31 + (c.codePointAt(0) ?? 0)) % 360, 7));
</script>

{#if src}<img class={cls} {src} alt="" />{:else}<span class={cls} style:--hue={hue} data-initial={initial} aria-hidden="true"></span>{/if}
