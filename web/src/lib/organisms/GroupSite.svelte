<script lang="ts">
  import Address from '#lib/atoms/Address.svelte';
  import ConfirmDialog from '#lib/molecules/ConfirmDialog.svelte';
  import Empty from '#lib/molecules/Empty.svelte';
  import FormDialog from '#lib/molecules/FormDialog.svelte';
  import { day } from '#lib/dates.ts';
  import { m } from '#lib/paraglide/messages.js';
  import { ParaglideMessage } from '@inlang/paraglide-js-svelte';
  import type { Site } from '#lib/server/site.ts';
  /**
   * A group's website: where it is served, how many files it has and
   * when it was published, to open or delete (asked to be sure of); and
   * the dialog publishing a folder in place of it, opened by a button
   * whose `popovertarget` is `publish-site`, a page for every address,
   * what of the group's its pages may show, and what they may write for
   * whoever signs in, under Advanced, as the site has them now.
   * `address`, the name it is served at.
   */
  let { address, site }: { address: string; site: Site | null } = $props();
  // What of the group's a site's pages may read, in the reader's words.
  // Who is looking, a page may always ask.
  const reads: [string, () => string][] = [
    ['fyi.opensocial.getGroup', m.group_site_read_group],
    ['fyi.opensocial.listMembers', m.group_site_read_members],
    ['fyi.opensocial.listSpaceRecords', m.group_site_read_records],
    ['fyi.opensocial.getSpaceBlob', m.group_site_read_blobs],
    ['fyi.opensocial.getActor', m.group_site_read_actors]
  ];
  // What a site's pages may do for whoever signs in, in the reader's words.
  const acts: [string, () => string][] = [
    ['fyi.opensocial.createReport', m.group_site_act_report],
    ['fyi.opensocial.requestJoin', m.group_site_act_join],
    ['fyi.opensocial.cancelJoinRequest', m.group_site_act_cancel],
    ['fyi.opensocial.leaveGroup', m.group_site_act_leave]
  ];
  // A space as a person types it: its type, and `/<key>` when it names one.
  const spaceText = (s: { space: string; key?: string }) =>
    s.key === undefined ? s.space : `${s.space}/${s.key}`;
  // Each file's path under the folder chosen, as the browser gives them,
  // in the files' order: a form sends each file by its name alone.
  let paths = $state('[]');
</script>

{#if site}
  <ul class="sites">
    <li>
      <span class="row-main">
        <strong class="mono"><Address {address} /></strong>
        <span class="what"
          >{m.group_site_files({ count: site.files, date: day(site.publishedAt) })}</span
        >
      </span>
      <span class="row-actions"
        ><a class="button quiet small" href={site.url} aria-label={m.group_site_open_label()}
          >{m.group_site_open()}</a
        ><button
          class="quiet small"
          type="button"
          popovertarget="delete-site"
          aria-label={m.group_site_delete_label()}>{m.common_delete()}</button
        ></span
      >
      <ConfirmDialog
        id="delete-site"
        title={m.group_site_delete_title()}
        action="?/unsite"
        confirm={m.common_delete()}
      >
        <ParaglideMessage message={m.group_site_delete_body}>
          {#snippet place()}<span class="mono">{address}</span>{/snippet}
        </ParaglideMessage>
      </ConfirmDialog>
    </li>
  </ul>
{:else}
  <Empty title={m.group_site_empty_title()}>{m.group_site_empty_body()}</Empty>
{/if}
<FormDialog
  id="publish-site"
  title={m.group_site_publish_title()}
  action="?/site"
  submit={m.group_site_publish()}
  multipart
>
  {#snippet intro()}{site
      ? m.group_site_publish_intro_replacing()
      : m.group_site_publish_intro()}{/snippet}
  <label class="field"
    >{m.group_site_folder()}
    <input
      name="files"
      type="file"
      multiple
      webkitdirectory
      required
      onchange={(e) =>
        (paths = JSON.stringify(
          [...(e.currentTarget.files ?? [])].map((f) => f.webkitRelativePath || f.name)
        ))}
    /></label
  >
  <input type="hidden" name="paths" value={paths} />
  <details
    class="more"
    open={!!(
      site?.reads.length ||
      site?.acts.length ||
      site?.spaces.length ||
      site?.writes.length ||
      site?.fallback
    )}
  >
    <summary>{m.group_site_advanced()}</summary>
    <label class="field"
      ><span>{m.group_site_fallback()} <span class="optional">{m.common_optional()}</span></span>
      <input
        name="fallback"
        maxlength="512"
        placeholder="index.html"
        value={site?.fallback ?? ''}
      /></label
    >
    <fieldset class="choices">
      <legend>{m.group_site_reads_legend()}</legend>
      <p class="mute">{m.group_site_reads_body()}</p>
      {#each reads as [method, words] (method)}<label
          ><input
            type="checkbox"
            name="reads"
            value={method}
            checked={site?.reads.includes(method) ?? false}
          />
          {words()}</label
        >{/each}
    </fieldset>
    <fieldset class="choices">
      <legend>{m.group_site_spaces_legend()}</legend>
      <p class="mute">{m.group_site_spaces_body()}</p>
      <label class="field"
        ><span>{m.group_site_spaces()} <span class="optional">{m.common_optional()}</span></span>
        <textarea
          name="spaces"
          rows="2"
          class="mono"
          spellcheck="false"
          placeholder="fyi.opensocial.siteSpace/board"
          >{site?.spaces.map(spaceText).join('\n') ?? ''}</textarea
        ></label
      >
    </fieldset>
    <fieldset class="choices">
      <legend>{m.group_site_acts_legend()}</legend>
      <p class="mute">{m.group_site_acts_body()}</p>
      {#each acts as [method, words] (method)}<label
          ><input
            type="checkbox"
            name="acts"
            value={method}
            checked={site?.acts.includes(method) ?? false}
          />
          {words()}</label
        >{/each}
    </fieldset>
    <fieldset class="choices">
      <legend>{m.group_site_writes_legend()}</legend>
      <p class="mute">{m.group_site_writes_body()}</p>
      <label class="field"
        ><span>{m.group_site_writes()} <span class="optional">{m.common_optional()}</span></span>
        <textarea
          name="writes"
          rows="3"
          class="mono"
          spellcheck="false"
          placeholder="fyi.opensocial.siteSpace/board:com.example.note"
          >{site?.writes.map((w) => `${spaceText(w)}:${w.collection}`).join('\n') ?? ''}</textarea
        ></label
      >
    </fieldset>
  </details>
</FormDialog>
