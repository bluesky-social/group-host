<script lang="ts">
  import type { Snippet } from 'svelte';
  import Avatar from '#lib/atoms/Avatar.svelte';
  import Tag from '#lib/atoms/Tag.svelte';
  /**
   * Someone in a list of a group's people: their picture; their handle (or
   * DID, when no handle is known), and under it the roles they hold beyond
   * being a member, which all do, and `note`, what they said; then
   * `children`, what may be done about them, at the row's end.
   */
  let {
    actor,
    roles = [],
    note,
    children
  }: {
    actor: { did: string; handle?: string };
    roles?: string[];
    note?: string;
    children?: Snippet;
  } = $props();
  const held = $derived(roles.filter((r) => r !== 'member'));
</script>

<Avatar name={actor.handle ?? actor.did} />
<span class="row-main">
  <span class="who mono">{actor.handle ?? actor.did}</span>
  {#if held.length}<span class="tags"
      >{#each held as role (role)}<Tag text={role} />{/each}</span
    >{/if}
  {#if note}<span class="what">{note}</span>{/if}
</span>
{#if children}<span class="row-actions">{@render children()}</span>{/if}
