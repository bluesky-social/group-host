<script lang="ts">
  import { m } from '#lib/paraglide/messages.js';
  import MoveSignIn from '#lib/molecules/MoveSignIn.svelte';
  import AccountMove from '#lib/organisms/AccountMove.svelte';
  import SignIn from '#lib/molecules/SignIn.svelte';
  import Page from '#lib/templates/Page.svelte';
  let { data, form } = $props();
</script>

<svelte:head><title>{m.route_move_head()}</title></svelte:head>

<Page title={m.route_move_title()}>
  {#if !data.actor}
    <p>{m.route_move_sign_in_first()}</p>
    <p><SignIn /></p>
  {:else if !data.actor.canCreateGroup}
    <p>{m.route_move_not_allowed()}</p>
  {:else if !data.moving}
    <p>{m.route_move_intro()}</p>
    <MoveSignIn back="/move" />
  {:else}
    <AccountMove moving={data.moving} plan={data.plan} failed={form?.failed} />
  {/if}
</Page>
