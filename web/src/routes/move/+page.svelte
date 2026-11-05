<script lang="ts">
  import MoveSignIn from '#lib/molecules/MoveSignIn.svelte';
  import AccountMove from '#lib/organisms/AccountMove.svelte';
  import SignIn from '#lib/molecules/SignIn.svelte';
  import Page from '#lib/templates/Page.svelte';
  let { data, form } = $props();
</script>

<svelte:head><title>Move an account here · opensocial</title></svelte:head>

<Page title="Move an account here">
  {#if !data.actor}
    <p>Sign in first. You'll run the group it becomes.</p>
    <p><SignIn /></p>
  {:else if !data.actor.canCreateGroup}
    <p>This host starts groups only for the people it names.</p>
  {:else if !data.moving}
    <p>
      An account you already have, on any server, can move here to become a group you run. It keeps
      its DID, so whoever follows it still does, and its spaces come with it.
    </p>
    <MoveSignIn back="/move" />
  {:else}
    <AccountMove moving={data.moving} plan={data.plan} failed={form?.failed} />
  {/if}
</Page>
