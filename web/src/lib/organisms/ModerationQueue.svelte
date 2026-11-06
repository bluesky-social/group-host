<script lang="ts">
  import { enhance } from '$app/forms';
  import Avatar from '#lib/atoms/Avatar.svelte';
  import Tag from '#lib/atoms/Tag.svelte';
  import Dialog from '#lib/molecules/Dialog.svelte';
  import Empty from '#lib/molecules/Empty.svelte';
  import Person from '#lib/molecules/Person.svelte';
  import { day, when } from '#lib/dates.ts';
  import { inDialog } from '#lib/forms.ts';
  import { reason } from '#lib/reasons.ts';
  type Actor = { did: string; handle?: string };
  type Said = {
    $type?: string;
    reasonType?: string;
    reason?: string;
    rule?: string;
    status?: string;
    note?: string;
    val?: string;
    deleted?: boolean;
  };
  type Event = { id: string; createdAt: string; creator?: Actor; event: Said };
  type Subject = {
    key: string;
    actor: Actor;
    status: string;
    reportCount: number;
    events: Event[];
  };
  /**
   * A group's moderation queue of one status: each subject by whose it is,
   * what it is, why it was reported and what was last said of it; what
   * happened to it, folded under it; for one who may, resolved or
   * escalated, with a note, from a dialog. `rules`, the group's rules'
   * titles by record key, for the reports that cite one.
   */
  let {
    subjects,
    status,
    canResolve,
    rules = {}
  }: {
    subjects: Subject[];
    status: string;
    canResolve: boolean;
    rules?: Record<string, string>;
  } = $props();
  // What fills each status's list, said where it is empty.
  const EMPTY: Record<string, string> = {
    open: 'New reports show up here.',
    escalated: 'What you escalate shows up here.',
    resolved: 'What you resolve shows up here.'
  };
  // Why a subject's dialog was refused, said in it.
  let failed = $state<Record<string, string | null>>({});

  const kindOf = (e: Event) => (e.event.$type ?? '').split('#')[1] ?? '';
  const reports = (s: Subject) => s.events.filter((e) => kindOf(e) === 'reportEvent');
  /** What a subject is: an account, or a record by the last word of its collection. */
  function what(key: string): string {
    if (key.startsWith('did:')) return 'Account';
    const kind = key.split('/').at(-2)?.split('.').at(-1) ?? 'record';
    return kind[0].toUpperCase() + kind.slice(1);
  }
  /** How often it was reported, and when last. */
  function reported(s: Subject): string {
    if (!s.reportCount) return 'Not reported';
    const times =
      s.reportCount === 1 ? 'once' : s.reportCount === 2 ? 'twice' : `${s.reportCount} times`;
    const last = reports(s).at(-1);
    if (!last) return `Reported ${times}`;
    return s.reportCount === 1
      ? `Reported on ${day(last.createdAt)}`
      : `Reported ${times}, last on ${day(last.createdAt)}`;
  }
  /** The reasons it was reported for, each once. */
  const reasons = (s: Subject) => [
    ...new Set(reports(s).map((e) => reason(e.event.reasonType ?? '')))
  ];
  /** A rule a report cites, by its title where the group's rules have it. */
  function rule(uri: string): string {
    const title = rules[uri.split('/').at(-1) ?? ''];
    return title ? `Breaks “${title}”` : 'Breaks a rule';
  }
  /** What an event was, in a word or two. */
  function title(e: Said, kind: string): string {
    if (kind === 'reportEvent') return 'Reported';
    if (kind === 'statusEvent') return e.status === 'escalated' ? 'Escalated' : 'Resolved';
    if (kind === 'labelEvent') return e.deleted ? 'Label removed' : 'Labelled';
    return 'Removed from the group';
  }
  /** What was said with an event, if anything. */
  function detail(e: Said, kind: string): string[] {
    if (kind === 'reportEvent')
      return [reason(e.reasonType ?? ''), e.rule ? rule(e.rule) : '', e.reason ?? ''];
    if (kind === 'labelEvent') return [e.val ?? ''];
    return [e.note ?? ''];
  }
</script>

{#if subjects.length}
  <ul class="queue">
    {#each subjects as s, i (s.key)}
      {@const latest = reports(s).at(-1)?.event.reason}
      <li class="subject">
        <Avatar name={s.actor.handle ?? s.actor.did} />
        <span class="row-main">
          <span class="who mono">{s.actor.handle ?? s.actor.did}</span>
          <span class="what">{what(s.key)} · {reported(s)}</span>
          {#if reasons(s).length}<span class="tags"
              >{#each reasons(s) as r (r)}<Tag text={r} />{/each}</span
            >{/if}
        </span>
        {#if canResolve}
          <span class="row-actions"
            ><button
              class="quiet small"
              type="button"
              popovertarget={`resolve-${i}`}
              aria-label={`Review ${s.actor.handle ?? s.actor.did}`}>Review</button
            ></span
          >
          <Dialog
            id={`resolve-${i}`}
            title="Resolve or escalate"
            onclose={() => (failed[s.key] = null)}
          >
            <p><Person actor={s.actor} /> <span class="mute">· {what(s.key)}</span></p>
            <p class="mute">
              Resolve it once it's dealt with, or escalate it for someone else to look at.
            </p>
            <form
              class="stack"
              method="post"
              action="?/resolve"
              use:enhance={inDialog(`resolve-${i}`, (why) => (failed[s.key] = why))}
            >
              <input type="hidden" name="subject" value={s.key} />
              <label class="field">A note, if you like <input name="note" maxlength="2000" /></label
              >
              {#if failed[s.key]}<p class="danger">{failed[s.key]}</p>{/if}
              <div class="actions">
                <button
                  class="quiet"
                  type="button"
                  popovertarget={`resolve-${i}`}
                  popovertargetaction="hide">Cancel</button
                >
                <button class="quiet" type="submit" name="status" value="escalated">Escalate</button
                >
                <button type="submit" name="status" value="resolved">Resolve</button>
              </div>
            </form>
          </Dialog>
        {/if}
        {#if latest}<p class="what note">{latest}</p>{/if}
        {#if s.events.length}
          <details class="history">
            <summary>History</summary>
            <ol class="events">
              {#each s.events as e (e.id)}
                {@const kind = kindOf(e)}
                {@const said = detail(e.event, kind).filter(Boolean)}
                <li>
                  <span
                    ><strong>{title(e.event, kind)}</strong>{#if e.creator}{' '}by <Person
                        actor={e.creator}
                      />{/if}</span
                  >
                  <span class="what">{when(e.createdAt)}</span>
                  {#if said.length}<span class="said">{said.join('. ')}</span>{/if}
                </li>
              {/each}
            </ol>
          </details>
        {/if}
      </li>
    {/each}
  </ul>
{:else}
  <Empty title={`Nothing ${status}`}>{EMPTY[status] ?? 'Reports show up here.'}</Empty>
{/if}
