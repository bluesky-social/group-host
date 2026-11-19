<script>
  import Empty from '../molecules/Empty.svelte';
  import NoteCard from '../molecules/NoteCard.svelte';
  import AppealDialog from './AppealDialog.svelte';
  import ReportDialog from './ReportDialog.svelte';
  /**
   * The board's notes, as a grid of cards, the newest first: each with its
   * author's handle (`handleOf`), shown as the group's labels on it say
   * (`judgedOf`), and what the viewer may do to it: their own note
   * deleted (`mine`, `del`), or appealed if the moderators marked it
   * (`appeal`); anyone else's reported (`report`, citing one of `rules`).
   * A moderator (`reviewOf`, where one reviews a note, or none) sees what
   * is hidden, and reviews what was marked in the console. With no notes,
   * what fills it.
   */
  let { notes, handleOf, judgedOf, mine, del, report, appeal, rules, reviewOf = null } = $props();
  /** The note a dialog is open for, and which. */
  let on = $state(null);
  let reporting = $state(false);
  let appealing = $state(false);
  const open = (note, which) => {
    on = note;
    if (which === 'report') reporting = true;
    else appealing = true;
  };
  /** What the moderators did to a note, as its author is told. */
  const didTo = (judged) =>
    judged.names.length
      ? `marked your note ${judged.names.join(', ')}`
      : 'hid your note from members';
</script>

{#if notes.length}
  <ul class="notes">
    {#each notes as note (note.uri)}
      {@const judged = judgedOf(note)}
      <NoteCard {note} handle={handleOf(note.repo)} {judged} sees={mine(note) || !!reviewOf}>
        {#snippet items(done)}
          {#if mine(note)}
            {#if judged.any}<button
                type="button"
                class="menu-item"
                role="menuitem"
                onclick={() => (done(), open(note, 'appeal'))}>Appeal</button
              >{/if}
            <button
              type="button"
              class="menu-item danger"
              role="menuitem"
              onclick={() => (done(), del(note))}>Delete</button
            >
          {:else}
            <button
              type="button"
              class="menu-item"
              role="menuitem"
              onclick={() => (done(), open(note, 'report'))}>Report</button
            >
          {/if}
          {#if reviewOf && judged.any}<a
              class="menu-item"
              role="menuitem"
              href={reviewOf(note.uri)}
              target="_blank"
              rel="noopener"
              onclick={done}>Review in the console</a
            >{/if}
        {/snippet}
      </NoteCard>
    {/each}
  </ul>
{:else}
  <Empty title="Nothing on the board"
    >Pin the first note: something to know, to ask for, or to give.</Empty
  >
{/if}
<ReportDialog
  bind:open={reporting}
  report={(reasonType, more) => report(on, reasonType, more)}
  {rules}
  reviewAt={reviewOf && on ? reviewOf(on.uri) : null}
/>
<AppealDialog
  bind:open={appealing}
  appeal={(reason) => appeal(on, reason)}
  did={on ? didTo(judgedOf(on)) : ''}
/>
