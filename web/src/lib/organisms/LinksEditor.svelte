<script lang="ts">
  import { enhance } from '$app/forms';
  import { inPlace } from '#lib/forms.ts';
  import Empty from '#lib/molecules/Empty.svelte';
  import FormDialog from '#lib/molecules/FormDialog.svelte';
  import Address from '#lib/atoms/Address.svelte';
  import LinkIcon from '#lib/atoms/LinkIcon.svelte';
  import { m } from '#lib/paraglide/messages.js';
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
          ><strong
            >{#if l.title}{l.title}{:else}<Address address={shown(l.url)} />{/if}</strong
          ><span class="what"><Address address={l.url} /></span></span
        >
        <form class="row-actions" method="post" action="?/unlink" use:enhance={inPlace}>
          <input type="hidden" name="index" value={i} />
          <button
            class="quiet small"
            type="submit"
            aria-label={m.links_editor_remove_label({ link: l.title ?? shown(l.url) })}
            >{m.common_remove()}</button
          >
        </form>
      </li>
    {/each}
  </ul>
{:else}
  <Empty title={m.links_editor_empty_title()}>{m.links_editor_empty_body()}</Empty>
{/if}
<FormDialog
  id="add-link"
  title={m.links_editor_add_title()}
  action="?/link"
  submit={m.common_add()}
>
  <label class="field"
    >{m.links_editor_address()}
    <input name="url" type="url" required placeholder="https://" /></label
  >
  <label class="field"
    ><span>{m.links_editor_link_title()} <span class="optional">{m.common_optional()}</span></span>
    <input
      name="title"
      maxlength="64"
      placeholder={m.links_editor_link_title_placeholder()}
    /></label
  >
</FormDialog>
