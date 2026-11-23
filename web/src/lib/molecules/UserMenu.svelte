<script lang="ts">
  import { m } from '#lib/paraglide/messages.js';
  import Avatar from '#lib/atoms/Avatar.svelte';
  import { SIGN_IN } from '#lib/signIn.ts';
  import { page } from '$app/state';
  /**
   * Who is signed in, as their picture and handle, opening a menu of
   * what is theirs: the other accounts this browser is signed in to
   * (`accounts`), each switched to in a click (`/console/switch`, the
   * host's, back to this page), and adding another, by the sign-in
   * dialog; their invites, how many are waiting (`invites`), as a dot on
   * their picture too while any are; their groups, starting one, moving
   * an account here, and, last, signing out (`/console/logout`), which
   * goes on as the account used before. It closes on a click elsewhere,
   * on Escape, and on going to another page.
   */
  let {
    handle,
    did,
    invites = 0,
    accounts = []
  }: {
    handle?: string;
    did: string;
    invites?: number;
    accounts?: { did: string; handle?: string }[];
  } = $props();
  const name = $derived(handle ? `@${handle}` : did);
  const named = (a: { did: string; handle?: string }) => (a.handle ? `@${a.handle}` : a.did);
  const here = $derived(`${page.url.pathname}${page.url.search}`);
  let menu: HTMLDetailsElement;
  function away(e: Event) {
    if (menu?.open && !menu.contains(e.target as Node)) menu.open = false;
  }
  function key(e: KeyboardEvent) {
    if (e.key === 'Escape' && menu?.open) {
      menu.open = false;
      menu.querySelector('summary')?.focus();
    }
  }
  const close = () => (menu.open = false);
</script>

<svelte:document onclick={away} onkeydown={key} />

<details class="user-menu" bind:this={menu}>
  <summary
    aria-label={invites
      ? m.user_menu_label_waiting({ name, count: invites })
      : m.user_menu_label({ name })}
  >
    <span class="waiting" class:dot={invites > 0}><Avatar name={handle ?? did} /></span><span
      class="handle mono">{name}</span
    >
  </summary>
  <div class="menu">
    {#if accounts.length}<p class="menu-label">{m.user_menu_switch_account()}</p>{/if}
    {#each accounts as account (account.did)}
      <form method="post" action="/console/switch">
        <input type="hidden" name="did" value={account.did} />
        <input type="hidden" name="back" value={here} />
        <button
          class="account"
          type="submit"
          aria-label={m.user_menu_switch_to({ account: named(account) })}
          ><Avatar name={account.handle ?? account.did} size="sm" /><span
            class="handle mono"
            title={named(account)}>{named(account)}</span
          ></button
        >
      </form>
    {/each}
    <button type="button" popovertarget={SIGN_IN} onclick={close}
      >{m.user_menu_add_account()}</button
    >
    <hr />
    <a href="/#invites" onclick={close}
      >{m.user_menu_invites()}{#if invites}<span class="badge">{invites}</span>{/if}</a
    >
    <a href="/" onclick={close}>{m.user_menu_your_groups()}</a>
    <a href="/new" onclick={close}>{m.user_menu_start_group()}</a>
    <a href="/move" onclick={close}>{m.user_menu_move()}</a>
    <hr />
    <form method="post" action="/console/logout">
      <button class="quiet" type="submit"
        >{accounts.length ? m.user_menu_sign_out_this() : m.user_menu_sign_out()}</button
      >
    </form>
  </div>
</details>
