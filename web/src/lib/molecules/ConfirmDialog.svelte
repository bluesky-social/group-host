<script lang="ts">
  import { m } from '#lib/paraglide/messages.js';
  import type { Snippet } from 'svelte';
  import { enhance } from '$app/forms';
  import Dialog from '#lib/molecules/Dialog.svelte';
  import { inDialog } from '#lib/forms.ts';
  /**
   * Asking to be sure of what cannot be undone, opened by a button whose
   * `popovertarget` is `id`: `children`, what will happen, in a sentence;
   * Cancel; and `confirm`, the act, sent to `action` with `fields`. One
   * that can be undone, though it should not be done lightly, is not
   * `danger`, and its act is the dialog's one primary button.
   */
  let {
    id,
    title,
    action,
    confirm,
    fields = {},
    danger = true,
    children
  }: {
    id: string;
    title: string;
    action: string;
    confirm: string;
    fields?: Record<string, string>;
    danger?: boolean;
    children: Snippet;
  } = $props();
  let failed = $state<string | null>(null);
</script>

<Dialog {id} {title} onclose={() => (failed = null)}>
  <p class="mute">{@render children()}</p>
  {#if failed}<p class="danger">{failed}</p>{/if}
  <form method="post" {action} class="actions" use:enhance={inDialog(id, (why) => (failed = why))}>
    {#each Object.entries(fields) as [name, value] (name)}<input
        type="hidden"
        {name}
        {value}
      />{/each}
    <button class="quiet" type="button" popovertarget={id} popovertargetaction="hide"
      >{m.common_cancel()}</button
    >
    <button class={danger ? 'danger' : undefined} type="submit">{confirm}</button>
  </form>
</Dialog>
