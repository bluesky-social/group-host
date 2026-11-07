<script lang="ts">
  import { enhance } from '$app/forms';
  import ConfirmDialog from '#lib/molecules/ConfirmDialog.svelte';
  import Dialog from '#lib/molecules/Dialog.svelte';
  import Person from '#lib/molecules/Person.svelte';
  import SubjectHistory from '#lib/molecules/SubjectHistory.svelte';
  import ReportDialog from '#lib/organisms/ReportDialog.svelte';
  import { inDialog, inOpenDialog } from '#lib/forms.ts';
  import { type Subject, TAKEDOWN, labelsOf, latestCid, reported, what } from '#lib/moderation.ts';
  /**
   * Everything a moderator does about one subject, in a dialog opened by a
   * button whose `popovertarget` is `id`: for an account, or a record
   * whose version the queue knows, the labels it
   * wears, each removable, those the group uses (`choices`) to add and
   * another; taking it down; then resolving or escalating it, or, for one
   * the queue has never had, reporting it; and what happened to it. Each
   * part only for one whose roles let them (`can`). `rules`, the group's
   * rules' titles by record key; `ruleList`, each rule's URI and title, to
   * cite; `reasons`, the report reasons the group takes. `opened`, open as
   * the page is drawn; `onclose`, told when it closes.
   */
  let {
    id,
    subject: s,
    choices,
    can,
    rules,
    ruleList,
    reasons,
    opened = false,
    onclose
  }: {
    id: string;
    subject: Subject;
    choices: string[];
    can: { label: boolean; takedown: boolean; resolve: boolean };
    rules: Record<string, string>;
    ruleList: { uri: string; title: string }[];
    reasons: string[];
    opened?: boolean;
    onclose?: () => void;
  } = $props();
  // Why something in the dialog was refused, said in it.
  let failed = $state<string | null>(null);
  const account = $derived(s.key.startsWith('did:'));
  // What a label is put on: the account, or the record at the version
  // last reported or labelled; none for a record of no known version.
  const target = $derived<Record<string, string> | null>(
    account
      ? { did: s.key }
      : latestCid(s.events)
        ? { uri: s.key, cid: latestCid(s.events)! }
        : null
  );
  const wearing = $derived(labelsOf(s.events));
  const takenDown = $derived(wearing.some((v) => TAKEDOWN.includes(v)));
  const toAdd = $derived(choices.filter((v) => !wearing.includes(v)));
  /** Whether the viewer may take `val` off: a takedown's only with takedown. */
  const removable = (val: string) => (TAKEDOWN.includes(val) ? can.takedown : can.label);
</script>

{#snippet which()}
  {#each Object.entries(target ?? {}) as [name, value] (name)}<input
      type="hidden"
      {name}
      {value}
    />{/each}
{/snippet}

<Dialog
  {id}
  title="Review"
  {opened}
  onclose={() => {
    failed = null;
    onclose?.();
  }}
>
  <p><Person actor={s.actor} /> <span class="mute">· {what(s.key)} · {reported(s)}</span></p>
  {#if failed}<p class="danger">{failed}</p>{/if}

  {#if target && (can.label || can.takedown)}
    <div class="review-part">
      <h3>Labels</h3>
      {#if wearing.length}
        <ul class="labels">
          {#each wearing as val (val)}
            <li class="label">
              <span class="mono">{val}</span>
              {#if removable(val)}
                <form
                  method="post"
                  action="?/unlabel"
                  use:enhance={inOpenDialog((w) => (failed = w))}
                >
                  {@render which()}
                  <input type="hidden" name="val" value={val} />
                  <button class="quiet small" type="submit" aria-label={`Remove the label ${val}`}
                    >×</button
                  >
                </form>
              {/if}
            </li>
          {/each}
        </ul>
      {:else}
        <p class="mute">No labels yet.</p>
      {/if}
      {#if can.label}
        {#if toAdd.length}<p class="mute">Labels the group has used, to add with a press:</p>{/if}
        <form
          class="label-choices"
          method="post"
          action="?/label"
          use:enhance={inOpenDialog((w) => (failed = w))}
        >
          {@render which()}
          {#each toAdd as val (val)}<button
              class="quiet small mono"
              type="submit"
              name="val"
              value={val}>{val}</button
            >{/each}
        </form>
        <form
          class="row-form"
          method="post"
          action="?/label"
          use:enhance={inOpenDialog((w) => (failed = w))}
        >
          {@render which()}
          <label class="field"
            >Add a label
            <input name="val" required maxlength="128" placeholder="off-topic" /></label
          >
          <button class="quiet" type="submit">Add</button>
        </form>
        <p class="mute">Apps that follow the group's labels decide what each one does.</p>
      {/if}
    </div>
    {#if can.takedown && !takenDown}
      <div class="review-part">
        <div class="row-form">
          {#if account}<p class="mute">
              Asks apps that follow the group's labels to hide everything this account posted. They
              stay in the group.
            </p>{:else}<p class="mute">
              Asks apps that follow the group's labels to hide this. The rest of what they posted
              stays.
            </p>{/if}
          <button class="quiet small" type="button" popovertarget={`${id}-takedown`}
            >Take down</button
          >
        </div>
        {#if account}
          <ConfirmDialog
            id={`${id}-takedown`}
            title="Take this account down?"
            action="?/label"
            confirm="Take down"
            danger={false}
            fields={{ ...target, val: '!takedown' }}
            >Apps that follow the group's labels hide what <Person actor={s.actor} /> posted. Removing
            the label undoes it.</ConfirmDialog
          >
        {:else}
          <ConfirmDialog
            id={`${id}-takedown`}
            title="Take this down?"
            action="?/label"
            confirm="Take down"
            danger={false}
            fields={{ ...target, val: '!takedown' }}
            >Apps that follow the group's labels hide this {what(s.key).toLowerCase()} by <Person
              actor={s.actor}
            />. Removing the label undoes it.</ConfirmDialog
          >
        {/if}
      </div>
    {/if}
  {:else if !target}
    <p class="mute">
      The queue doesn't know which version of this to label. Label it from a terminal, with
      opensocial mod label and its CID.
    </p>
  {/if}

  {#if s.status && can.resolve}
    <div class="review-part">
      <h3>Decision</h3>
      <form
        class="stack"
        method="post"
        action="?/resolve"
        use:enhance={inDialog(id, (w) => (failed = w))}
      >
        <input type="hidden" name="subject" value={s.key} />
        <label class="field">A note, if you like <input name="note" maxlength="2000" /></label>
        <div class="actions">
          <button class="quiet" type="button" popovertarget={id} popovertargetaction="hide"
            >Cancel</button
          >
          <button class="quiet" type="submit" name="status" value="escalated">Escalate</button>
          <button type="submit" name="status" value="resolved">Resolve</button>
        </div>
      </form>
    </div>
  {:else if !s.status && account}
    <div class="review-part">
      <h3>Decision</h3>
      <p class="mute">
        Nobody has reported them. Report them to put them in the queue, for you or another moderator
        to decide.
      </p>
      <div class="actions">
        <button class="quiet" type="button" popovertarget={`${id}-report`}
          >Report this account</button
        >
      </div>
      <ReportDialog id={`${id}-report`} actor={s.actor} {reasons} rules={ruleList} />
    </div>
  {/if}

  <SubjectHistory events={s.events} {rules} open />
</Dialog>
