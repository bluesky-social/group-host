<script lang="ts">
  import { m } from '#lib/paraglide/messages.js';
  import { type Reviewed, filesOf, reviewedFile, wordsOf } from '#lib/moderation.ts';
  /**
   * A record as a moderator reviews it: what it says, field by field, and
   * the pictures and files it holds, each read through the queue, which
   * serves them though the group took the record down; marked as taken
   * down when it is, as only moderators then see it. The whole record
   * folded under it.
   */
  let { record }: { record: Reviewed } = $props();
  const words = $derived(wordsOf(record.value));
  const files = $derived(filesOf(record.value));
</script>

<div class="reviewed">
  {#if record.takenDown}
    <p class="taken-down-note">
      <span class="tag down">{m.reviewed_record_taken_down()}</span>
      <span class="mute small">{m.reviewed_record_taken_down_note()}</span>
    </p>
  {/if}
  {#if words.length}
    <dl class="record">
      {#each words as w (w.name)}<dt class="mono">{w.name}</dt>
        <dd>{w.text}</dd>{/each}
    </dl>
  {/if}
  {#each files as f (f.cid)}
    {#if f.mimeType.startsWith('image/')}<img
        class="record-picture"
        src={reviewedFile(record.uri, f.cid)}
        alt={m.reviewed_record_picture({ field: f.field })}
        loading="lazy"
      />{:else}<a href={reviewedFile(record.uri, f.cid)} download
        >{m.reviewed_record_file({ field: f.field, kind: f.mimeType || '?' })}</a
      >{/if}
  {/each}
  <details class="more">
    <summary>{m.reviewed_record_whole()}</summary>
    <pre class="raw">{JSON.stringify(record.value, null, 2)}</pre>
  </details>
</div>
