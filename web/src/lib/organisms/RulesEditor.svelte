<script lang="ts">
  import ConfirmDialog from '#lib/molecules/ConfirmDialog.svelte';
  import Empty from '#lib/molecules/Empty.svelte';
  import FormDialog from '#lib/molecules/FormDialog.svelte';
  import { m } from '#lib/paraglide/messages.js';
  /**
   * A group's rules, in order, each to delete (asked to be sure of);
   * and the dialog adding one, opened by a button whose `popovertarget`
   * is `add-rule`.
   */
  let { rules = [] }: { rules?: { rkey: string; title: string; text?: string }[] } = $props();
</script>

{#if rules.length}
  <ul class="rule-rows">
    {#each rules as r, i (r.rkey)}
      <li>
        <span class="ordinal">{i + 1}</span>
        <span class="row-main"
          ><strong>{r.title}</strong>{#if r.text}<span class="what">{r.text}</span>{/if}</span
        >
        <span class="row-actions"
          ><button
            class="quiet small"
            type="button"
            popovertarget={`unrule-${i}`}
            aria-label={m.rules_editor_delete_label({ rule: r.title })}>{m.common_delete()}</button
          ></span
        >
        <ConfirmDialog
          id={`unrule-${i}`}
          title={m.rules_editor_delete_title()}
          action="?/unrule"
          confirm={m.common_delete()}
          fields={{ rkey: r.rkey }}
        >
          {m.rules_editor_delete_body({ rule: r.title })}
        </ConfirmDialog>
      </li>
    {/each}
  </ul>
{:else}
  <Empty title={m.rules_editor_empty_title()}>{m.rules_editor_empty_body()}</Empty>
{/if}
<FormDialog
  id="add-rule"
  title={m.rules_editor_add_title()}
  action="?/rule"
  submit={m.common_add()}
>
  <label class="field"
    >{m.rules_editor_rule()}
    <input
      name="title"
      required
      maxlength="64"
      placeholder={m.rules_editor_rule_placeholder()}
    /></label
  >
  <label class="field"
    ><span>{m.rules_editor_meaning()} <span class="optional">{m.common_optional()}</span></span>
    <textarea name="text" maxlength="1000"></textarea></label
  >
</FormDialog>
