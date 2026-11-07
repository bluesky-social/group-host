<script lang="ts">
  import ConfirmDialog from '#lib/molecules/ConfirmDialog.svelte';
  import Empty from '#lib/molecules/Empty.svelte';
  import FormDialog from '#lib/molecules/FormDialog.svelte';
  type Defined = {
    identifier: string;
    severity: string;
    blurs: string;
    defaultSetting?: string;
    locales: { name: string; description: string }[];
  };
  /**
   * The labels a group defines, each by its name and value, then what it
   * means and how apps show it; each to delete (asked to be sure of); and
   * the dialog creating one, opened by a button whose `popovertarget` is
   * `create-label`.
   */
  let { labels = [] }: { labels?: Defined[] } = $props();
  const SHOWN: Record<string, string> = {
    hide: 'Apps hide it until someone chooses to see it.',
    warn: 'Apps show it with a warning.',
    ignore: 'Apps show it as is.'
  };
  const COVERED: Record<string, string> = {
    content: 'They cover all of it.',
    media: 'They cover its pictures.'
  };
  /** What it means, and how apps show what wears it, in sentences. */
  const said = (l: Defined) =>
    [l.locales[0]?.description, SHOWN[l.defaultSetting ?? 'warn'], COVERED[l.blurs]]
      .filter(Boolean)
      .join(' ');
</script>

{#if labels.length}
  <ul class="label-rows">
    {#each labels as l, i (l.identifier)}
      {@const name = l.locales[0]?.name ?? l.identifier}
      <li>
        <span class="row-main"
          ><span><strong>{name}</strong> <span class="tag worn">{l.identifier}</span></span>
          <span class="what">{said(l)}</span></span
        >
        <span class="row-actions"
          ><button
            class="quiet small"
            type="button"
            popovertarget={`delete-label-${i}`}
            aria-label={`Delete the label ${l.identifier}`}>Delete</button
          ></span
        >
        <ConfirmDialog
          id={`delete-label-${i}`}
          title="Delete this label?"
          action="?/undefine"
          confirm="Delete"
          fields={{ label: l.identifier }}
        >
          Moderators can no longer choose “{name}”. What already wears it keeps it.
        </ConfirmDialog>
      </li>
    {/each}
  </ul>
{:else}
  <Empty title="No labels yet"
    >A label says what moderators found, such as spam or off-topic, so apps can warn about it or
    hide it.</Empty
  >
{/if}
<FormDialog id="create-label" title="Create a label" action="?/define" submit="Create">
  {#snippet intro()}Moderators pick it when they review an account or post. Creating one with a
    value already in use changes that label.{/snippet}
  <label class="field"
    >Name <input name="name" required maxlength="64" placeholder="Off-topic" /></label
  >
  <label class="field"
    >Value <input
      name="label"
      required
      maxlength="100"
      pattern="[a-z\-]+"
      placeholder="off-topic"
    /></label
  >
  <p class="mute">Lowercase letters and hyphens.</p>
  <label class="field"
    >What it means <textarea name="description" required maxlength="2000"></textarea></label
  >
  <label class="field"
    >How apps show it <select name="defaultSetting">
      <option value="warn">With a warning</option>
      <option value="hide">Hidden until someone chooses to see it</option>
      <option value="ignore">As is</option>
    </select></label
  >
  <label class="field"
    >What apps cover <select name="blurs">
      <option value="none">Nothing</option>
      <option value="media">Its pictures</option>
      <option value="content">All of it</option>
    </select></label
  >
  <input type="hidden" name="severity" value="inform" />
</FormDialog>
