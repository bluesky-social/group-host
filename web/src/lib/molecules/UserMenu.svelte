<script lang="ts">
  import Avatar from '#lib/atoms/Avatar.svelte';
  /**
   * Who is signed in, as their picture and handle, opening a menu of
   * what is theirs: their invites, how many are waiting (`invites`), as a
   * dot on their picture too while any are; their groups, starting one,
   * moving an account here, and, last, signing out (`/console/logout`,
   * the host's). It closes on a click elsewhere, on Escape, and on going
   * to another page.
   */
  let { handle, did, invites = 0 }: { handle?: string; did: string; invites?: number } = $props();
  const name = $derived(handle ? `@${handle}` : did);
  const waiting = $derived(invites === 1 ? '1 invite waiting' : `${invites} invites waiting`);
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
  <summary aria-label={invites ? `${name}: your menu, ${waiting}` : `${name}: your menu`}>
    <span class="waiting" class:dot={invites > 0}><Avatar name={handle ?? did} /></span><span
      class="handle mono">{name}</span
    >
  </summary>
  <div class="menu">
    <a href="/#invites" onclick={close}
      >Invites{#if invites}<span class="badge">{invites}</span>{/if}</a
    >
    <a href="/" onclick={close}>Your groups</a>
    <a href="/new" onclick={close}>Start a group</a>
    <a href="/move" onclick={close}>Move an account here</a>
    <hr />
    <form method="post" action="/console/logout">
      <button class="quiet" type="submit">Sign out</button>
    </form>
  </div>
</details>
