<script lang="ts">
  import Address from '#lib/atoms/Address.svelte';
  import ConfirmDialog from '#lib/molecules/ConfirmDialog.svelte';
  import Empty from '#lib/molecules/Empty.svelte';
  import FormDialog from '#lib/molecules/FormDialog.svelte';
  import { day } from '#lib/dates.ts';
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
  const reads = [
    ['fyi.opensocial.getGroup', "The group's profile and rules"],
    ['fyi.opensocial.listMembers', 'Who the members are'],
    ['fyi.opensocial.listSpaceRecords', "Posts in the group's spaces"],
    ['fyi.opensocial.getSpaceBlob', 'Pictures and files in those posts'],
    ['fyi.opensocial.getActor', "People's profiles"]
  ];
  // What a site's pages may do for whoever signs in, in the reader's words.
  const acts = [
    ['fyi.opensocial.createReport', 'Report posts and members to moderators'],
    ['fyi.opensocial.requestJoin', 'Join the group, or ask to join'],
    ['fyi.opensocial.cancelJoinRequest', 'Cancel their request to join'],
    ['fyi.opensocial.leaveGroup', 'Leave the group']
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
          >{site.files === 1 ? '1 file' : `${site.files} files`}, published {day(
            site.publishedAt
          )}</span
        >
      </span>
      <span class="row-actions"
        ><a class="button quiet small" href={site.url} aria-label="Open the site">Open</a><button
          class="quiet small"
          type="button"
          popovertarget="delete-site"
          aria-label="Delete the site">Delete</button
        ></span
      >
      <ConfirmDialog id="delete-site" title="Delete this site?" action="?/unsite" confirm="Delete">
        Its pages at <span class="mono">{address}</span> go offline. Anyone who opens that address sees
        the group's page here instead.
      </ConfirmDialog>
    </li>
  </ul>
{:else}
  <Empty title="Nothing published yet">Publish a folder of web pages to put them online.</Empty>
{/if}
<FormDialog id="publish-site" title="Publish a site" action="?/site" submit="Publish" multipart>
  {#snippet intro()}{`Choose the folder with your site's pages. Put the front page at the top, named index.html, and a 404.html beside it for addresses that aren't found.${site ? " This replaces the site that's there now." : ''}`}{/snippet}
  <label class="field"
    >Folder
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
    <summary>Advanced</summary>
    <label class="field"
      ><span
        >Send every address to one page, for single-page apps <span class="optional"
          >(optional)</span
        ></span
      >
      <input
        name="fallback"
        maxlength="512"
        placeholder="index.html"
        value={site?.fallback ?? ''}
      /></label
    >
    <fieldset class="choices">
      <legend>Let pages show the group's data</legend>
      <p class="mute">
        Pages written to ask for these can show them to people who sign in, as far as the group lets
        each person see. Other pages stay as they are.
      </p>
      {#each reads as [method, words] (method)}<label
          ><input
            type="checkbox"
            name="reads"
            value={method}
            checked={site?.reads.includes(method) ?? false}
          />
          {words}</label
        >{/each}
    </fieldset>
    <fieldset class="choices">
      <legend>Let pages show every post in a space</legend>
      <p class="mute">
        Pages that ask for these spaces see every post in them. People who sign in see only what the
        group lets them see. A key after a slash picks one space of the kind.
      </p>
      <label class="field"
        ><span>Kinds of space, one per line <span class="optional">(optional)</span></span>
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
      <legend>Let pages act for people who sign in</legend>
      <p class="mute">Pages can do these for people who sign in, in their name.</p>
      {#each acts as [method, words] (method)}<label
          ><input
            type="checkbox"
            name="acts"
            value={method}
            checked={site?.acts.includes(method) ?? false}
          />
          {words}</label
        >{/each}
    </fieldset>
    <fieldset class="choices">
      <legend>Let pages write in the group's spaces</legend>
      <p class="mute">
        Pages written to make these can make them for people who sign in, in their own account, as
        far as the group lets each person write.
      </p>
      <label class="field"
        ><span>Kinds of record, one per line <span class="optional">(optional)</span></span>
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
