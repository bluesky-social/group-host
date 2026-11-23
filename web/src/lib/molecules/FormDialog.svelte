<script lang="ts">
  import { m } from '#lib/paraglide/messages.js';
  import type { Snippet } from 'svelte';
  import { enhance } from '$app/forms';
  import Dialog from '#lib/molecules/Dialog.svelte';
  import { inDialog } from '#lib/forms.ts';
  /**
   * An act that asks for input, in a dialog opened by a button whose
   * `popovertarget` is `id`: `intro`, a line on what it does; the form,
   * sent to `action`, its fields `children`; then Cancel, and `submit`,
   * the act. `wide` for a form with much in it. Sent in place: done, it
   * closes; refused, it stays open, saying why.
   */
  let {
    id,
    title,
    action,
    submit,
    intro,
    multipart = false,
    wide = false,
    children
  }: {
    id: string;
    title: string;
    action: string;
    submit: string;
    intro?: Snippet;
    multipart?: boolean;
    wide?: boolean;
    children: Snippet;
  } = $props();
  let failed = $state<string | null>(null);
</script>

<Dialog {id} {title} onclose={() => (failed = null)} {wide}>
  {#if intro}<p class="mute">{@render intro()}</p>{/if}
  <form
    class="stack"
    method="post"
    {action}
    enctype={multipart ? 'multipart/form-data' : undefined}
    use:enhance={inDialog(id, (why) => (failed = why))}
  >
    {@render children()}
    {#if failed}<p class="danger">{failed}</p>{/if}
    <div class="actions">
      <button class="quiet" type="button" popovertarget={id} popovertargetaction="hide"
        >{m.common_cancel()}</button
      >
      <button type="submit">{submit}</button>
    </div>
  </form>
</Dialog>
