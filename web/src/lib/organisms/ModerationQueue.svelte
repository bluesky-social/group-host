<script lang="ts">
  import { goto } from '$app/navigation';
  import { page } from '$app/state';
  import Avatar from '#lib/atoms/Avatar.svelte';
  import Tag from '#lib/atoms/Tag.svelte';
  import Empty from '#lib/molecules/Empty.svelte';
  import SubjectHistory from '#lib/molecules/SubjectHistory.svelte';
  import ReviewDialog from '#lib/organisms/ReviewDialog.svelte';
  import {
    type Subject,
    labelsOf,
    labelsUsed,
    reasons as reasonsOf,
    reported,
    reports,
    what
  } from '#lib/moderation.ts';
  /**
   * A group's moderation queue of one status: each subject by whose it is,
   * what it is, when and why it was reported, the labels it wears and what
   * the latest report said; what happened to it, folded under it; and, for
   * one whose roles let them act on it (`can`), a Review of it. `found`,
   * someone looked up who is not in this list, and `reviewing`, the
   * subject whose Review opens as the page is drawn. `rules`, the group's
   * rules' titles by record key; `ruleList`, each rule's URI and title;
   * `reasons`, the report reasons the group takes.
   */
  let {
    subjects,
    status,
    can,
    rules = {},
    ruleList = [],
    reasons = [],
    found = null,
    reviewing = null
  }: {
    subjects: Subject[];
    status: string;
    can: { label: boolean; takedown: boolean; resolve: boolean };
    rules?: Record<string, string>;
    ruleList?: { uri: string; title: string }[];
    reasons?: string[];
    found?: Subject | null;
    reviewing?: string | null;
  } = $props();
  // What fills each status's list, said where it is empty.
  const EMPTY: Record<string, string> = {
    open: 'New reports show up here.',
    escalated: 'What you escalate shows up here.',
    resolved: 'What you resolve shows up here.'
  };
  const acts = $derived(can.label || can.takedown || can.resolve);
  const choices = $derived(labelsUsed(found ? [...subjects, found] : subjects));
  /** The page without the subject it was asked for open, once that closes. */
  function closed(key: string) {
    if (key !== reviewing) return;
    const url = new URL(page.url.href);
    url.searchParams.delete('review');
    goto(url, { replace: true, reset: false });
  }
</script>

{#snippet review(s: Subject, id: string)}
  <ReviewDialog
    {id}
    subject={s}
    {choices}
    {can}
    {rules}
    {ruleList}
    {reasons}
    opened={s.key === reviewing}
    onclose={() => closed(s.key)}
  />
{/snippet}

{#if subjects.length}
  <ul class="queue">
    {#each subjects as s, i (s.key)}
      {@const latest = reports(s).at(-1)?.event.reason}
      {@const wearing = labelsOf(s.events)}
      <li class="subject">
        <Avatar name={s.actor.handle ?? s.actor.did} />
        <span class="row-main">
          <span class="who mono">{s.actor.handle ?? s.actor.did}</span>
          <span class="what">{what(s.key)} · {reported(s)}</span>
          {#if reasonsOf(s).length || wearing.length}<span class="tags"
              >{#each reasonsOf(s) as r (r)}<Tag text={r} />{/each}{#if wearing.length}<span
                  class="what labelled">Labelled</span
                >{/if}{#each wearing as val (val)}<span class="tag worn">{val}</span>{/each}</span
            >{/if}
        </span>
        {#if acts}
          <span class="row-actions"
            ><button
              class="quiet small"
              type="button"
              popovertarget={`review-${i}`}
              aria-label={`Review ${s.actor.handle ?? s.actor.did}`}>Review</button
            ></span
          >
          {@render review(s, `review-${i}`)}
        {/if}
        {#if latest}<p class="what note">{latest}</p>{/if}
        <SubjectHistory events={s.events} {rules} />
      </li>
    {/each}
  </ul>
{:else}
  <Empty title={`Nothing ${status}`}>{EMPTY[status] ?? 'Reports show up here.'}</Empty>
{/if}
{#if found && acts}{@render review(found, 'review-found')}{/if}
