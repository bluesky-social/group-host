<script>
  import Dialog from '../molecules/Dialog.svelte';
  import { sending } from '../forms.svelte.js';
  /**
   * Reporting a note or a post (`title`, the dialog's fixed words, such as
   * "Report this note") to the place's moderators: why, in atproto's
   * reasons and the visitor's own words, and which of the group's `rules`
   * it breaks, given to `report`. Sent, it says what comes of it, and to a
   * moderator, where to review it (`reviewAt`, a URL, or none).
   */
  let {
    open = $bindable(false),
    title = 'Report this note',
    report,
    rules = [],
    reviewAt = null
  } = $props();
  let sent = $state(false);
  const REASONS = [
    ['com.atproto.moderation.defs#reasonRude', 'Rude, or unkind'],
    ['com.atproto.moderation.defs#reasonSpam', 'Spam'],
    ['com.atproto.moderation.defs#reasonMisleading', 'Misleading'],
    ['com.atproto.moderation.defs#reasonViolation', 'Breaks one of the rules'],
    ['com.atproto.moderation.defs#reasonOther', 'Something else']
  ];
  const form = sending(async (data) => {
    const reason = String(data.get('reason') ?? '').trim();
    const rule = String(data.get('rule') ?? '');
    await report(String(data.get('reasonType')), { reason, rule });
    sent = true;
  });
  $effect(() => {
    if (!open) sent = false;
  });
</script>

<Dialog bind:open label={title}>
  <h2 class="dialog-title">{title}</h2>
  {#if sent}
    <p class="lede">
      Sent to the moderators. They see it in the place's queue, and decide what to do.
    </p>
    <div class="acts end">
      {#if reviewAt}<a class="button quiet" href={reviewAt} target="_blank" rel="noopener"
          >Review it in the console</a
        >{/if}
      <button type="button" onclick={() => (open = false)}>Done</button>
    </div>
  {:else}
    <p class="lede">Only the moderators see who reported it, and why.</p>
    <form class="form" onsubmit={form.send}>
      <label class="field"
        >Why
        <select name="reasonType">
          {#each REASONS as [value, label] (value)}<option {value}>{label}</option>{/each}
        </select></label
      >
      {#if rules.length}
        <label class="field"
          ><span>Which rule <span class="optional">(optional)</span></span>
          <select name="rule">
            <option value="">None in particular</option>
            {#each rules as rule (rule.uri)}<option value={rule.uri}>{rule.title}</option>{/each}
          </select></label
        >
      {/if}
      <label class="field"
        ><span>In your words <span class="optional">(optional)</span></span>
        <textarea name="reason" rows="3" maxlength="2000"></textarea></label
      >
      {#if form.failed}<p class="failed" role="status">{form.failed}</p>{/if}
      <div class="acts end">
        <button type="button" class="quiet" onclick={() => (open = false)}>Cancel</button>
        <button type="submit" disabled={form.busy}>Report</button>
      </div>
    </form>
  {/if}
</Dialog>
