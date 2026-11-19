<script>
  import MemberRow from '../molecules/MemberRow.svelte';
  /** The place's members, those who run it first, each with their card and picture (`people`). */
  let { people } = $props();
</script>

<ul class="rows people">
  {#each people as p (p.did)}<MemberRow
      handle={p.handle}
      roles={p.roles}
      card={p.card}
      avatar={p.avatar}
    />{/each}
</ul>
