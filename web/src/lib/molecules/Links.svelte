<script lang="ts">
  import LinkIcon from '#lib/atoms/LinkIcon.svelte';
  /** Where else a group is, as its profile lists them: each by its site's icon, and its title or its address. */
  let {
    name,
    links
  }: { name: string; links: { url: string; title?: string; icon?: string | null }[] } = $props();
  const shown = (url: string) => url.replace(/^https?:\/\//, '').replace(/\/$/, '');
</script>

{#if links.length}
  <nav class="profile-links" aria-label={`${name}'s links`}>
    {#each links as l, i (i)}
      <a href={l.url} rel="noopener noreferrer"
        ><LinkIcon src={l.icon} />{l.title ?? shown(l.url)}</a
      >
    {/each}
  </nav>
{/if}
