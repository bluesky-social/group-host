<script>
  import Avatar from '../atoms/Avatar.svelte';
  /**
   * A member of the place, as a row: their picture (`avatar`, its URL to
   * come, or none) and handle, the roles they hold beyond being a member,
   * and their card, if they wrote one; at its end, `act`, if any.
   */
  let { handle, roles, card, avatar = null, act } = $props();
  const NAMES = { admin: 'Admin', host: 'Host' };
  const held = $derived(roles.filter((r) => r !== 'member').map((r) => NAMES[r] ?? r));
</script>

<li>
  <Avatar {handle} load={avatar} />
  <span class="row-main">
    <span class="row-name">
      <strong>@{handle}</strong>
      {#if held.length}<span class="role">{held.join(', ')}</span>{/if}
    </span>
    {#if card?.about}<span>{card.about}</span>{/if}
    {#if card?.offers}<span class="what">Can help with: {card.offers}</span>{/if}
  </span>
  {#if act}{@render act()}{/if}
</li>
