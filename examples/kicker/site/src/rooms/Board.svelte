<script>
  import RoomHead from '../lib/molecules/RoomHead.svelte';
  import NoteBoard from '../lib/organisms/NoteBoard.svelte';
  import PinNoteDialog from '../lib/organisms/PinNoteDialog.svelte';
  import { follow, keep, kept } from '../lib/kept.svelte.js';
  import {
    LABEL,
    NS,
    appeal,
    create,
    handleOf,
    judged,
    keyOf,
    moderates,
    remove,
    report,
    reviewAt,
    rulesOf
  } from '../lib/place.js';
  import { labelsOn, newestFirst, ofCollection } from '../lib/views.js';
  /**
   * The board: members' notes in the place's room, each shown as the
   * group's labels on it say, and one hidden left out but for its author
   * and the moderators; pinning one; reporting another's to the
   * moderators, through the group host, and appealing what they did to
   * one's own. Moderators label and resolve in the group host's console.
   */
  let { ctx } = $props();
  let failed = $state('');

  async function load() {
    try {
      await keep(ctx, 'room');
    } catch (e) {
      failed = e.message;
    }
  }
  $effect(() => {
    void ctx;
    load();
    return follow(ctx, 'room');
  });

  const read = $derived(
    kept.room && {
      notes: newestFirst(ofCollection(kept.room, `${NS}.note`)),
      labels: labelsOn(kept.room, ctx.group.did, LABEL)
    }
  );
  const judgedOf = (n) => judged(ctx, read.labels.get(n.uri));
  const mine = (n) => n.repo === ctx.me.did;
  /** The notes the viewer sees: all but those hidden, unless theirs or they moderate. */
  const shown = $derived(
    (read?.notes ?? []).filter((n) => judgedOf(n).setting !== 'hide' || mine(n) || moderates(ctx))
  );
  /** Does `act`, then reads the board again. */
  const then =
    (act) =>
    async (...args) => {
      await act(...args);
      await load();
    };
</script>

<RoomHead title="Board">
  {#snippet act()}<PinNoteDialog
      pin={then((note) => create(ctx, 'room', 'note', note))}
    />{/snippet}
</RoomHead>
{#if failed}<p class="failed">{failed}</p>{/if}
{#if read}
  <NoteBoard
    notes={shown}
    handleOf={(did) => handleOf(ctx, did)}
    {judgedOf}
    {mine}
    del={then((n) => remove(ctx, 'room', 'note', keyOf(n.uri)))}
    report={(n, reasonType, more) => report(ctx, n, reasonType, more)}
    appeal={(n, reason) => appeal(ctx, n, reason)}
    rules={rulesOf(ctx)}
    reviewOf={moderates(ctx) ? (uri) => reviewAt(ctx, uri) : null}
  />
{:else}
  <p class="meta">Reading the board…</p>
{/if}
