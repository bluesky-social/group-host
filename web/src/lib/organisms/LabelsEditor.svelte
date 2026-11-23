<script lang="ts">
  import ConfirmDialog from '#lib/molecules/ConfirmDialog.svelte';
  import Empty from '#lib/molecules/Empty.svelte';
  import FormDialog from '#lib/molecules/FormDialog.svelte';
  import { localized, type LabelLocale } from '#lib/labels.ts';
  import { m } from '#lib/paraglide/messages.js';
  type Defined = {
    identifier: string;
    severity: string;
    blurs: string;
    defaultSetting?: string;
    locales: LabelLocale[];
  };
  /**
   * The labels a group defines, each by its name and value, then what it
   * means and how apps show it; each to delete (asked to be sure of); and
   * the dialog creating one, opened by a button whose `popovertarget` is
   * `create-label`.
   */
  let { labels = [] }: { labels?: Defined[] } = $props();
  const SHOWN: Record<string, () => string> = {
    hide: m.labels_editor_shown_hide,
    warn: m.labels_editor_shown_warn,
    ignore: m.labels_editor_shown_ignore
  };
  const COVERED: Record<string, () => string> = {
    content: m.labels_editor_covered_content,
    media: m.labels_editor_covered_media
  };
  /** What it means, and how apps show what wears it, in sentences. */
  const said = (l: Defined) =>
    [localized(l.locales)?.description, SHOWN[l.defaultSetting ?? 'warn']?.(), COVERED[l.blurs]?.()]
      .filter(Boolean)
      .join(' ');
</script>

{#if labels.length}
  <ul class="label-rows">
    {#each labels as l, i (l.identifier)}
      {@const name = localized(l.locales)?.name ?? l.identifier}
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
            aria-label={m.labels_editor_delete_label({ label: l.identifier })}
            >{m.common_delete()}</button
          ></span
        >
        <ConfirmDialog
          id={`delete-label-${i}`}
          title={m.labels_editor_delete_title()}
          action="?/undefine"
          confirm={m.common_delete()}
          fields={{ label: l.identifier }}
        >
          {m.labels_editor_delete_body({ name })}
        </ConfirmDialog>
      </li>
    {/each}
  </ul>
{:else}
  <Empty title={m.labels_editor_empty_title()}>{m.labels_editor_empty_body()}</Empty>
{/if}
<FormDialog
  id="create-label"
  title={m.labels_editor_create_title()}
  action="?/define"
  submit={m.common_create()}
>
  {#snippet intro()}{m.labels_editor_create_intro()}{/snippet}
  <label class="field"
    >{m.labels_editor_name()}
    <input
      name="name"
      required
      maxlength="64"
      placeholder={m.labels_editor_name_placeholder()}
    /></label
  >
  <label class="field"
    >{m.labels_editor_value()}
    <input
      name="label"
      required
      maxlength="100"
      pattern="[a-z\-]+"
      placeholder="off-topic"
    /></label
  >
  <p class="mute">{m.labels_editor_value_hint()}</p>
  <label class="field"
    >{m.labels_editor_description()}
    <textarea name="description" required maxlength="2000"></textarea></label
  >
  <label class="field"
    >{m.labels_editor_setting()}
    <select name="defaultSetting">
      <option value="warn">{m.labels_editor_setting_warn()}</option>
      <option value="hide">{m.labels_editor_setting_hide()}</option>
      <option value="ignore">{m.labels_editor_setting_ignore()}</option>
    </select></label
  >
  <label class="field"
    >{m.labels_editor_blurs()}
    <select name="blurs">
      <option value="none">{m.labels_editor_blurs_none()}</option>
      <option value="media">{m.labels_editor_blurs_media()}</option>
      <option value="content">{m.labels_editor_blurs_content()}</option>
    </select></label
  >
  <input type="hidden" name="severity" value="inform" />
</FormDialog>
