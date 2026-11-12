<script lang="ts">
  import Address from '#lib/atoms/Address.svelte';
  import ConfirmDialog from '#lib/molecules/ConfirmDialog.svelte';
  import Empty from '#lib/molecules/Empty.svelte';
  import FormDialog from '#lib/molecules/FormDialog.svelte';
  import { day } from '#lib/dates.ts';
  /**
   * A group's website: where it is served, how many files it has and
   * when it was published, to open or delete (asked to be sure of); and
   * the dialog publishing a folder in place of it, opened by a button
   * whose `popovertarget` is `publish-site`. `address`, the name it is
   * served at.
   */
  let {
    address,
    site
  }: { address: string; site: { url: string; files: number; publishedAt: string } | null } =
    $props();
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
  {#snippet intro()}{`Choose the folder with your site's pages. Put the front page at the top, named index.html.${site ? " This replaces the site that's there now." : ''}`}{/snippet}
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
  <label class="field"
    >Page for addresses that aren't found, if you like
    <input name="notFound" maxlength="512" placeholder="404.html" /></label
  >
  <label class="field"
    >One page for every address, if you like
    <input name="fallback" maxlength="512" placeholder="index.html, for an app" /></label
  >
</FormDialog>
