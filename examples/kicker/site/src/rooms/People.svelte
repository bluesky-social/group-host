<script>
  import RoomHead from '../lib/molecules/RoomHead.svelte';
  import CardDialog from '../lib/organisms/CardDialog.svelte';
  import PeopleList from '../lib/organisms/PeopleList.svelte';
  import { follow, keep, kept } from '../lib/kept.svelte.js';
  import { NS, leave, picture, put, upload } from '../lib/place.js';
  import { cardsByWriter, recordAt } from '../lib/views.js';
  /**
   * People: every member, those who run the place first, each with their
   * roles and the card they keep in its room, their picture with it;
   * editing the viewer's own, and leaving, after which the place is read
   * again (`ctx.reread`) and shows them its landing.
   */
  let { ctx } = $props();
  let failed = $state('');

  async function load() {
    try {
      await keep(ctx, 'room');
    } catch (e) {
      failed = e.message;
    }
  }
  $effect(() => {
    void ctx;
    load();
    return follow(ctx, 'room');
  });

  const cards = $derived(cardsByWriter(kept.room ?? [], `${NS}.card`));
  const people = $derived(
    [...ctx.members.entries()]
      .map(([did, m]) => {
        const card = cards.get(did);
        const avatar = card?.avatar ? picture(ctx, 'room', did, card.avatar) : null;
        return { did, ...m, card, avatar };
      })
      .sort((a, b) => b.roles.length - a.roles.length || a.handle.localeCompare(b.handle))
  );
  /**
   * Writes the viewer's card: its words, and its picture, `file` uploaded
   * to their own PDS in place of the one it had, or none if they `drop`
   * it; neither, it keeps the one it had.
   */
  const save = async (fields, file, drop) => {
    const was = recordAt(kept.room ?? [], ctx.me.did, `${NS}.card`, 'self');
    const avatar = file ? await upload(file) : drop ? null : was?.value.avatar;
    await put(ctx, 'room', 'card', 'self', avatar ? { ...fields, avatar } : fields, was);
    await load();
  };
</script>

<!-- The card's button once it is read, so nothing typed is written over as it comes in. -->
<RoomHead title="People">
  {#snippet act()}{#if kept.room}<CardDialog card={cards.get(ctx.me.did)} {save} />{/if}{/snippet}
</RoomHead>
{#if failed}<p class="failed">{failed}</p>{/if}
<PeopleList
  {people}
  me={ctx.me.did}
  name={ctx.place?.displayName ?? ctx.group.handle}
  leave={async () => {
    await leave();
    await ctx.reread();
  }}
/>
