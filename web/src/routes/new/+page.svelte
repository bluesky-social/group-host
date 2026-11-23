<script lang="ts">
  import { ParaglideMessage } from '@inlang/paraglide-js-svelte';
  import { m } from '#lib/paraglide/messages.js';
  import NewGroupForm from '#lib/organisms/NewGroupForm.svelte';
  import SignIn from '#lib/molecules/SignIn.svelte';
  import Page from '#lib/templates/Page.svelte';
  let { data, form } = $props();
</script>

<svelte:head><title>{m.route_new_head()}</title></svelte:head>

<Page title={m.route_new_title()}>
  {#if !data.actor}
    <p>{m.route_new_sign_in_first()}</p>
    <p><SignIn /></p>
  {:else if !data.actor.canCreateGroup}
    <p>{m.route_new_not_allowed()}</p>
  {:else}
    <NewGroupForm failed={form?.failed} domain={data.domain} />
    <p>
      <ParaglideMessage message={m.route_new_or_move}>
        {#snippet link({ children })}<a href="/move">{@render children?.()}</a>{/snippet}
      </ParaglideMessage>
    </p>
  {/if}
</Page>
