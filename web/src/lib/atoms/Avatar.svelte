<script lang="ts">
	/** A round picture of a group or a person, or a plain disc where there is none. */
	let { src, size }: { src?: string | null; size?: 'md' } = $props();
	let cls = $derived(size ? `av ${size}` : 'av');
</script>

{#if src}<img class={cls} {src} alt="" />{:else}<span class={cls} aria-hidden="true"></span>{/if}
