<script lang="ts">
	/** Where else a group is, as its profile lists them: each by its title, or its address. */
	let { name, links }: { name: string; links: { url: string; title?: string }[] } = $props();
	const shown = (url: string) => url.replace(/^https?:\/\//, '').replace(/\/$/, '');
</script>

{#if links.length}
	<nav class="profile-links" aria-label={`${name}'s links`}>
		{#each links as l, i (i)}
			<a href={l.url} rel="noopener noreferrer"><svg class="icon" viewBox="0 0 24 24" aria-hidden="true"><path d="M10 13a5 5 0 0 0 7.07 0l3-3a5 5 0 0 0-7.07-7.07l-1 1" /><path d="M14 11a5 5 0 0 0-7.07 0l-3 3a5 5 0 0 0 7.07 7.07l1-1" /></svg>{l.title ?? shown(l.url)}</a>
		{/each}
	</nav>
{/if}
