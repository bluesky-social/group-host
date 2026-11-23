<script lang="ts">
  import { ParaglideMessage } from '@inlang/paraglide-js-svelte';
  import Copy from '#lib/atoms/Copy.svelte';
  import ConfirmDialog from '#lib/molecules/ConfirmDialog.svelte';
  import FormDialog from '#lib/molecules/FormDialog.svelte';
  import { m } from '#lib/paraglide/messages.js';
  /**
   * The group's rotation keys, the highest ranked first: the stewards'
   * recovery keys, each to remove (asked to be sure of), and this
   * host's; and the dialog adding a key, opened by a button whose
   * `popovertarget` is `add-key`, with goat's command to make one. A
   * recovery key's holder can undo what this host does to the group's
   * identity, and take the group elsewhere without it.
   */
  type Key = { key: string; holder: string; addedBy?: string };
  let { keys, hostHolds }: { keys: Key[]; hostHolds: boolean } = $props();
  /** goat's command that makes a key pair, its public half a did:key. */
  const generate = 'goat key generate';
</script>

<ol class="recovery-keys">
  {#each keys as k, i (k.key)}
    <li>
      <span class="row-main">
        <code class="key">{k.key}</code>
        <span class="what"
          >{k.holder === 'host'
            ? m.recovery_keys_host()
            : k.addedBy
              ? m.recovery_keys_steward_added({ who: k.addedBy })
              : m.recovery_keys_steward()}</span
        >
      </span>
      {#if k.holder === 'steward' && hostHolds}
        <span class="row-actions"
          ><button class="quiet small" type="button" popovertarget={`unkey-${i}`}
            >{m.common_remove()}</button
          ></span
        >
        <ConfirmDialog
          id={`unkey-${i}`}
          title={m.recovery_keys_remove_title()}
          action="?/unkey"
          confirm={m.common_remove()}
          fields={{ key: k.key }}
        >
          {m.recovery_keys_remove_body()}
        </ConfirmDialog>
      {/if}
    </li>
  {/each}
</ol>
<FormDialog id="add-key" title={m.recovery_keys_add_title()} action="?/key" submit={m.common_add()}>
  {#snippet intro()}{m.recovery_keys_add_intro()}{/snippet}
  <p class="mute">
    <ParaglideMessage message={m.recovery_keys_add_goat}>
      {#snippet link({ children })}<a
          href="https://github.com/bluesky-social/goat"
          target="_blank"
          rel="noopener noreferrer">{@render children?.()}</a
        >{/snippet}
    </ParaglideMessage>
  </p>
  <div class="copyable"><code>{generate}</code><Copy text={generate} /></div>
  <label class="field"
    >{m.recovery_keys_key()} <input name="key" required placeholder="did:key:z…" /></label
  >
</FormDialog>
