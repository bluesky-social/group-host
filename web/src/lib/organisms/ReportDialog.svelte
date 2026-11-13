<script lang="ts">
  import FormDialog from '#lib/molecules/FormDialog.svelte';
  import Person from '#lib/molecules/Person.svelte';
  import type { Actor } from '#lib/moderation.ts';
  import { reason } from '#lib/reasons.ts';
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

<FormDialog {id} title="Report an account" action="?/report" submit="Report">
  {#snippet intro()}<Person {actor} /> goes into the open queue with your report.{/snippet}
  <input type="hidden" name="did" value={actor.did} />
  <fieldset class="reasons">
    <legend>Why</legend>
    {#each reasons as r, i (r)}<label
        ><input type="radio" name="reasonType" value={r} required checked={i === 0} />
        {reason(r)}</label
      >{/each}
  </fieldset>
  {#if rules.length}
    <label class="field"
      >The rule it breaks, if any
      <select name="rule">
        <option value="">None</option>
        {#each rules as r (r.uri)}<option value={r.uri}>{r.title}</option>{/each}
      </select></label
    >
  {/if}
  <label class="field"
    ><span>What happened <span class="optional">(optional)</span></span>
    <textarea name="reason" rows="3" maxlength="2000"></textarea></label
  >
</FormDialog>
