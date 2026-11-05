<script lang="ts">
	/** A link's site, by its icon where the group kept one, or a chain where it did not or the icon will not load. */
	let { src }: { src?: string | null } = $props();
	let failed = $state(false);
</script>

{#if src && !failed}
	<img class="link-icon" {src} alt="" onerror={() => (failed = true)} />
{:else}
	<svg class="icon" viewBox="0 0 24 24" aria-hidden="true"><path d="M10 13a5 5 0 0 0 7.07 0l3-3a5 5 0 0 0-7.07-7.07l-1 1" /><path d="M14 11a5 5 0 0 0-7.07 0l-3 3a5 5 0 0 0 7.07 7.07l1-1" /></svg>
{/if}
