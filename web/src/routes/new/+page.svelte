<script lang="ts">
  import NewGroupForm from '#lib/organisms/NewGroupForm.svelte';
  import SignIn from '#lib/molecules/SignIn.svelte';
  import Page from '#lib/templates/Page.svelte';
  let { data, form } = $props();
</script>

<svelte:head><title>Start a group · opensocial</title></svelte:head>

<Page title="Start a group">
  {#if !data.actor}
    <p>Sign in first. You'll be the group's founder.</p>
    <p><SignIn /></p>
  {:else if !data.actor.canCreateGroup}
    <p>This host starts groups only for the people it names.</p>
  {:else}
    <NewGroupForm failed={form?.failed} domain={data.domain} />
    <p>Or <a href="/move">move an account you already have</a> here, to become a group.</p>
  {/if}
</Page>
