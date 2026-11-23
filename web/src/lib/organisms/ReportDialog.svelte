<script lang="ts">
  import { ParaglideMessage } from '@inlang/paraglide-js-svelte';
  import FormDialog from '#lib/molecules/FormDialog.svelte';
  import Person from '#lib/molecules/Person.svelte';
  import type { Actor } from '#lib/moderation.ts';
  import { reason } from '#lib/reasons.ts';
  import { m } from '#lib/paraglide/messages.js';
  /**
   * Reporting an account as a moderator, into the group's open queue as a
   * member's report would go: why, from `reasons` (the reason types the
   * group takes), the rule it breaks, from `rules` (each rule's record URI
   * and title), and what happened. Opened by a button whose
   * `popovertarget` is `id`.
   */
  let {
    id,
    actor,
    reasons,
    rules
  }: {
    id: string;
    actor: Actor;
    reasons: string[];
    rules: { uri: string; title: string }[];
  } = $props();
</script>

<FormDialog
  {id}
  title={m.report_dialog_title()}
  action="?/report"
  submit={m.report_dialog_submit()}
>
  {#snippet intro()}<ParaglideMessage message={m.report_dialog_intro}>
      {#snippet person()}<Person {actor} />{/snippet}
    </ParaglideMessage>{/snippet}
  <input type="hidden" name="did" value={actor.did} />
  <fieldset class="reasons">
    <legend>{m.report_dialog_why()}</legend>
    {#each reasons as r, i (r)}<label
        ><input type="radio" name="reasonType" value={r} required checked={i === 0} />
        {reason(r)}</label
      >{/each}
  </fieldset>
  {#if rules.length}
    <label class="field"
      >{m.report_dialog_rule()}
      <select name="rule">
        <option value="">{m.report_dialog_no_rule()}</option>
        {#each rules as r (r.uri)}<option value={r.uri}>{r.title}</option>{/each}
      </select></label
    >
  {/if}
  <label class="field"
    ><span>{m.report_dialog_what()} <span class="optional">{m.common_optional()}</span></span>
    <textarea name="reason" rows="3" maxlength="2000"></textarea></label
  >
</FormDialog>
