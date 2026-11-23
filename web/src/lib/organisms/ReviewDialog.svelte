<script lang="ts">
  import { enhance } from '$app/forms';
  import ConfirmDialog from '#lib/molecules/ConfirmDialog.svelte';
  import Dialog from '#lib/molecules/Dialog.svelte';
  import Person from '#lib/molecules/Person.svelte';
  import SubjectHistory from '#lib/molecules/SubjectHistory.svelte';
  import ReportDialog from '#lib/organisms/ReportDialog.svelte';
  import { inDialog, inOpenDialog } from '#lib/forms.ts';
  import { type Subject, TAKEDOWN, labelsOf, latestCid, reported, what } from '#lib/moderation.ts';
  import { m } from '#lib/paraglide/messages.js';
  import { ParaglideMessage } from '@inlang/paraglide-js-svelte';
  /**
   * Everything a moderator does about one subject, in a dialog opened by a
   * button whose `popovertarget` is `id`: for an account, or a record
   * whose version the queue knows, the labels it
   * wears, each removable, those the group defines or has used
   * (`choices`, each its value and what it is called) to add and
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
    choices: { val: string; name: string }[];
    can: { label: boolean; resolve: boolean };
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
  const toAdd = $derived(choices.filter((c) => !wearing.includes(c.val)));
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
  title={m.review_dialog_title()}
  {opened}
  onclose={() => {
    failed = null;
    onclose?.();
  }}
>
  <p><Person actor={s.actor} /> <span class="mute">· {what(s.key)} · {reported(s)}</span></p>
  {#if failed}<p class="danger">{failed}</p>{/if}

  {#if target && can.label}
    <div class="review-part">
      <h3>{m.review_dialog_labels()}</h3>
      {#if wearing.length}
        <ul class="labels">
          {#each wearing as val (val)}
            <li class="label">
              <span class="mono">{val}</span>
              <form
                method="post"
                action="?/unlabel"
                use:enhance={inOpenDialog((w) => (failed = w))}
              >
                {@render which()}
                <input type="hidden" name="val" value={val} />
                <button
                  class="quiet small"
                  type="submit"
                  aria-label={m.review_dialog_remove_label({ label: val })}>×</button
                >
              </form>
            </li>
          {/each}
        </ul>
      {:else}
        <p class="mute">{m.review_dialog_no_labels()}</p>
      {/if}
      {#if toAdd.length}<p class="mute">{m.review_dialog_press_label()}</p>{/if}
      <form
        class="label-choices"
        method="post"
        action="?/label"
        use:enhance={inOpenDialog((w) => (failed = w))}
      >
        {@render which()}
        {#each toAdd as c (c.val)}<button
            class="quiet small"
            class:mono={c.name === c.val}
            type="submit"
            name="val"
            value={c.val}
            aria-label={m.review_dialog_add_label_named({ label: c.val })}>{c.name}</button
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
          >{m.review_dialog_add_label()}
          <input name="val" required maxlength="128" placeholder="off-topic" /></label
        >
        <button class="quiet" type="submit">{m.common_add()}</button>
      </form>
      <p class="mute">{m.review_dialog_labels_hint()}</p>
    </div>
    {#if !takenDown}
      <div class="review-part">
        <div class="row-form">
          {#if account}<p class="mute">{m.review_dialog_takedown_account_hint()}</p>{:else}<p
              class="mute"
            >
              {m.review_dialog_takedown_hint()}
            </p>{/if}
          <button class="quiet small" type="button" popovertarget={`${id}-takedown`}
            >{m.review_dialog_takedown()}</button
          >
        </div>
        {#if account}
          <ConfirmDialog
            id={`${id}-takedown`}
            title={m.review_dialog_takedown_account_title()}
            action="?/label"
            confirm={m.review_dialog_takedown()}
            danger={false}
            fields={{ ...target, val: '!takedown' }}
            ><ParaglideMessage message={m.review_dialog_takedown_account_body}>
              {#snippet person()}<Person actor={s.actor} />{/snippet}
            </ParaglideMessage></ConfirmDialog
          >
        {:else}
          <ConfirmDialog
            id={`${id}-takedown`}
            title={m.review_dialog_takedown_record_title()}
            action="?/label"
            confirm={m.review_dialog_takedown()}
            danger={false}
            fields={{ ...target, val: '!takedown' }}
            ><ParaglideMessage
              message={m.review_dialog_takedown_record_body}
              inputs={{ kind: what(s.key).toLowerCase() }}
            >
              {#snippet person()}<Person actor={s.actor} />{/snippet}
            </ParaglideMessage></ConfirmDialog
          >
        {/if}
      </div>
    {/if}
  {:else if !target}
    <p class="mute">{m.review_dialog_no_version({ command: 'opensocial mod label' })}</p>
  {/if}

  {#if s.status && can.resolve}
    <div class="review-part">
      <h3>{m.review_dialog_decision()}</h3>
      <form
        class="stack"
        method="post"
        action="?/resolve"
        use:enhance={inDialog(id, (w) => (failed = w))}
      >
        <input type="hidden" name="subject" value={s.key} />
        <label class="field"
          ><span>{m.review_dialog_note()} <span class="optional">{m.common_optional()}</span></span>
          <input name="note" maxlength="2000" /></label
        >
        <div class="actions">
          <button class="quiet" type="button" popovertarget={id} popovertargetaction="hide"
            >{m.common_cancel()}</button
          >
          <button class="quiet" type="submit" name="status" value="escalated"
            >{m.review_dialog_escalate()}</button
          >
          <button type="submit" name="status" value="resolved">{m.review_dialog_resolve()}</button>
        </div>
      </form>
    </div>
  {:else if !s.status && account}
    <div class="review-part">
      <h3>{m.review_dialog_decision()}</h3>
      <p class="mute">{m.review_dialog_unreported()}</p>
      <div class="actions">
        <button class="quiet" type="button" popovertarget={`${id}-report`}
          >{m.review_dialog_report()}</button
        >
      </div>
      <ReportDialog id={`${id}-report`} actor={s.actor} {reasons} rules={ruleList} />
    </div>
  {/if}

  <SubjectHistory events={s.events} {rules} open />
</Dialog>
