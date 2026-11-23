<script lang="ts">
  import ConfirmDialog from '#lib/molecules/ConfirmDialog.svelte';
  import Empty from '#lib/molecules/Empty.svelte';
  import FormDialog from '#lib/molecules/FormDialog.svelte';
  import Person from '#lib/molecules/Person.svelte';
  import { day } from '#lib/dates.ts';
  import { m } from '#lib/paraglide/messages.js';
  import { ParaglideMessage } from '@inlang/paraglide-js-svelte';
  /**
   * The group's app passwords, never the passwords, each to revoke (asked
   * to be sure of); a new one, shown once (`made`); and the dialog making
   * one, opened by a button whose `popovertarget` is `make-password`.
   */
  type Password = {
    name: string;
    member: string;
    actor?: { did: string; handle?: string };
    privileged: boolean;
    lastUsedAt?: string;
  };
  let { passwords, made }: { passwords: Password[]; made?: { name: string; password: string } } =
    $props();
  // Who made it, whether it may chat, and when it was last used, in a line.
  const madeBy = (p: Password) =>
    p.lastUsedAt
      ? p.privileged
        ? m.app_passwords_by_chat_used
        : m.app_passwords_by_used
      : p.privileged
        ? m.app_passwords_by_chat
        : m.app_passwords_by;
</script>

{#if made}
  <p class="made">
    <ParaglideMessage message={m.app_passwords_made} inputs={{ name: made.name }}>
      {#snippet password()}<code class="password">{made.password}</code>{/snippet}
    </ParaglideMessage>
  </p>
{/if}
{#if passwords.length}
  <ul class="passwords">
    {#each passwords as p, i (p.name)}
      <li>
        <span class="row-main">
          <strong>{p.name}</strong>
          <span class="what"
            ><ParaglideMessage
              message={madeBy(p)}
              inputs={{ date: p.lastUsedAt ? day(p.lastUsedAt) : '' }}
            >
              {#snippet person()}<Person actor={p.actor ?? { did: p.member }} />{/snippet}
            </ParaglideMessage></span
          >
        </span>
        <span class="row-actions"
          ><button
            class="quiet small"
            type="button"
            popovertarget={`unpassword-${i}`}
            aria-label={m.app_passwords_revoke_label({ name: p.name })}>{m.common_revoke()}</button
          ></span
        >
        <ConfirmDialog
          id={`unpassword-${i}`}
          title={m.app_passwords_revoke_title()}
          action="?/unpassword"
          confirm={m.common_revoke()}
          fields={{ name: p.name }}
        >
          {m.app_passwords_revoke_body({ name: p.name })}
        </ConfirmDialog>
      </li>
    {/each}
  </ul>
{:else}
  <Empty title={m.app_passwords_empty_title()}>{m.app_passwords_empty_body()}</Empty>
{/if}
<FormDialog
  id="make-password"
  title={m.app_passwords_make_title()}
  action="?/password"
  submit={m.common_create()}
>
  {#snippet intro()}{m.app_passwords_make_intro()}{/snippet}
  <label class="field"
    >{m.app_passwords_where()}
    <input
      name="name"
      required
      maxlength="64"
      placeholder={m.app_passwords_where_placeholder()}
    /></label
  >
  <label><input type="checkbox" name="privileged" /> {m.app_passwords_privileged()}</label>
</FormDialog>
