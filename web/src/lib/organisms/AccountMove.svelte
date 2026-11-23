<script lang="ts">
  import { ParaglideMessage } from '@inlang/paraglide-js-svelte';
  import { m } from '#lib/paraglide/messages.js';
  /**
   * An account signed in to move here: before its move starts, what
   * moving it would make of it, and its address here to choose; once
   * started, the code its server emailed, to finish. `failed` is what
   * the host said of the last try, if it refused it.
   */
  type Moving = { did: string; handle?: string; pds?: string };
  type Plan = { handle: string; label: string; keeps: boolean; refused?: unknown };
  let { moving, plan, failed }: { moving: Moving; plan: Plan | null; failed?: string } = $props();
  const name = $derived(moving.handle ?? moving.did);
</script>

{#snippet account()}<span class="mono">@{name}</span>{/snippet}

<div class="stack">
  {#if failed}<p class="danger">{failed}</p>{/if}
  {#if plan}
    <p>
      <ParaglideMessage message={m.account_move_moving} {account}>
        {#snippet server()}<span class="mono">{moving.pds}</span>{/snippet}
      </ParaglideMessage>
    </p>
    {#if plan.refused}
      <p class="danger">{m.account_move_taken()}</p>
    {/if}
    <p>
      <ParaglideMessage message={plan.keeps ? m.account_move_here_own : m.account_move_here}>
        {#snippet handle()}<span class="mono">@{plan.handle}</span>{/snippet}
      </ParaglideMessage>
    </p>
    <form class="stack" method="get">
      <label class="field">
        {m.account_move_address()}
        <input name="address" value={plan.label} maxlength="63" pattern="[A-Za-z0-9\-]+" />
      </label>
      <button class="quiet" type="submit">{m.account_move_check()}</button>
    </form>
    <form class="stack" method="post" action="?/start">
      <input type="hidden" name="address" value={plan.label} />
      <p>{m.account_move_copy_body()}</p>
      <button type="submit" disabled={!!plan.refused}>{m.account_move_copy()}</button>
    </form>
  {:else}
    <p><ParaglideMessage message={m.account_move_copied} {account} /></p>
    <form class="stack" method="post" action="?/finish">
      <label class="field"
        >{m.account_move_code()}
        <input name="code" required autocomplete="one-time-code" maxlength="64" /></label
      >
      <button type="submit">{m.account_move_finish()}</button>
    </form>
  {/if}
  <form method="post" action="?/cancel">
    <button class="quiet" type="submit">{m.account_move_cancel()}</button>
  </form>
</div>
