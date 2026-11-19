<script>
  import Handle from '../atoms/Handle.svelte';
  import Menu from './Menu.svelte';
  import { ago } from '../dates.js';
  /**
   * A note on the board: what kind it is, who pinned it and when, in a
   * line, with the menu of what may be done to it (`items`, as Menu takes
   * them) across from it; then its words. As the group's labels on it say
   * (`judged`): hidden from members, a line saying so, for those still
   * shown it; folded behind what it was marked, until opened, but for its
   * author and the moderators (`sees`); or shown, with what it was marked.
   */
  let {
    note,
    handle,
    judged = { setting: 'show', names: [], any: false },
    sees = false,
    items
  } = $props();
  const KINDS = { notice: 'Notice', ask: 'Ask', offer: 'Offer' };
  let opened = $state(false);
  const marked = $derived(judged.names.join(', '));
  const folded = $derived(judged.setting === 'warn' && !sees && !opened);
</script>

<li class="note" class:judged={judged.any}>
  <div class="note-head">
    <span class="meta"
      >{KINDS[note.value.kind] ?? 'Notice'} from <Handle {handle} />, {ago(
        note.value.createdAt
      )}</span
    >
    {#if items}<Menu label="What to do with this note" {items} />{/if}
  </div>
  {#if judged.setting === 'hide'}<p class="judgement">
      Hidden from members by the moderators{#if marked}: <strong>{marked}</strong>{:else}.{/if}
    </p>{:else if marked}<p class="judgement">
      Marked by the moderators: <strong>{marked}</strong>
    </p>{/if}
  {#if folded}
    <button type="button" class="quiet small" onclick={() => (opened = true)}>Show it</button>
  {:else}
    <p class="note-text">{note.value.text}</p>
  {/if}
</li>
