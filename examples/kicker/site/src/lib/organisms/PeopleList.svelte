<script>
  import MemberRow from '../molecules/MemberRow.svelte';
  import LeaveDialog from './LeaveDialog.svelte';
  /**
   * The place's members, those who run it first, each with their card and
   * picture (`people`); on the viewer's own row (`me`, their DID), leaving
   * the place (`name`) by `leave`.
   */
  let { people, me, name, leave } = $props();
</script>

<ul class="rows people">
  {#each people as p (p.did)}<MemberRow
      handle={p.handle}
      roles={p.roles}
      card={p.card}
      avatar={p.avatar}
      >{#snippet act()}{#if p.did === me}<LeaveDialog {name} {leave} />{/if}{/snippet}</MemberRow
    >{/each}
</ul>
