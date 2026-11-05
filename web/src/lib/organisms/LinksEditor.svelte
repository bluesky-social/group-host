<script lang="ts">
  import { enhance } from '$app/forms';
  import { inPlace } from '#lib/forms.ts';
  import Empty from '#lib/molecules/Empty.svelte';
  import FormDialog from '#lib/molecules/FormDialog.svelte';
  import LinkIcon from '#lib/atoms/LinkIcon.svelte';
  /**
   * Where else a group is, as its profile lists them, each by its site's
   * icon, to remove;
   * and the dialog adding one, opened by a button whose `popovertarget`
   * is `add-link`.
   */
  let { links = [] }: { links?: { url: string; title?: string; icon?: string | null }[] } =
    $props();
  const shown = (url: string) => url.replace(/^https?:\/\//, '').replace(/\/$/, '');
</script>

{#if links.length}
  <ul class="link-list">
    {#each links as l, i (i)}
      <li>
        <LinkIcon src={l.icon} />
        <span class="row-main"
          ><strong>{l.title ?? shown(l.url)}</strong><span class="what">{l.url}</span></span
        >
        <form class="row-actions" method="post" action="?/unlink" use:enhance={inPlace}>
          <input type="hidden" name="index" value={i} />
          <button class="quiet small" type="submit" aria-label={`Remove ${l.title ?? shown(l.url)}`}
            >Remove</button
          >
        </form>
      </li>
    {/each}
  </ul>
{:else}
  <Empty title="No links yet">Other places to find the group, like its chat or website.</Empty>
{/if}
<FormDialog id="add-link" title="Add a link" action="?/link" submit="Add">
  <label class="field"
    >Its address <input name="url" type="url" required placeholder="https://" /></label
  >
  <label class="field"
    >Title, if you like <input name="title" maxlength="64" placeholder="Our chat" /></label
  >
</FormDialog>
