<script lang="ts">
  import Icon from '#lib/atoms/Icon.svelte';
  /**
   * A "⋯" menu of what can be done about the row it ends, for a list
   * whose every row carries the same acts: each a button opening the
   * dialog `opens` names. `label` names the menu for those who cannot see
   * it ("More for @bob.test"). It closes on a pick, a click elsewhere, and
   * Escape.
   */
  let { actions, label }: { actions: { text: string; opens: string }[]; label: string } = $props();
  let menu = $state<HTMLDetailsElement>();
  function away(e: Event) {
    if (menu?.open && !menu.contains(e.target as Node)) menu.open = false;
  }
  function key(e: KeyboardEvent) {
    if (e.key === 'Escape' && menu?.open) {
      menu.open = false;
      menu.querySelector('summary')?.focus();
    }
  }
</script>

<svelte:document onclick={away} onkeydown={key} />

{#if actions.length}
  <details class="action-menu" bind:this={menu}>
    <summary aria-label={label} title={label}><Icon name="more" /></summary>
    <div class="menu">
      {#each actions as a (a.opens)}<button
          type="button"
          popovertarget={a.opens}
          onclick={() => menu && (menu.open = false)}>{a.text}</button
        >{/each}
    </div>
  </details>
{/if}
