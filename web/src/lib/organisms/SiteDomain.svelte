<script lang="ts">
  import { enhance } from '$app/forms';
  import Empty from '#lib/molecules/Empty.svelte';
  import FormDialog from '#lib/molecules/FormDialog.svelte';
  import { inPlace } from '#lib/forms.ts';
  /**
   * Where a group's site is served, from its own domain's side: with no
   * `domain` of its own (its handle is its `address`), the way to take
   * one, under `profile`; with one, whether the site is there (`on`), to
   * move it there, by the dialog opened by a button whose
   * `popovertarget` is `site-domain`, or back. `host` is the name whose
   * addresses the domain's DNS records point at.
   */
  let {
    address,
    domain,
    on,
    host,
    profile
  }: { address: string; domain: string | null; on: boolean; host: string; profile: string } =
    $props();
</script>

{#if !domain}
  <Empty title="No domain yet">
    Change the group's handle to a domain you own. Then you can move its site there.
    {#snippet action()}<a class="button quiet small" href={profile}>Change the handle</a>{/snippet}
  </Empty>
{:else}
  <ul class="sites">
    <li>
      <span class="row-main">
        <strong class="mono">{domain}</strong>
        {#if on}
          <span class="what"
            >Your site is at <span class="mono">{domain}</span>.
            <span class="mono">{address}</span> sends visitors there.</span
          >
        {:else}
          <span class="what">Your site is at <span class="mono">{address}</span> for now.</span>
        {/if}
      </span>
      <span class="row-actions">
        {#if on}
          <form method="post" action="?/undomain" use:enhance={inPlace}>
            <button class="quiet small" type="submit" aria-label="Move the site back to its address"
              >Move back</button
            >
          </form>
        {:else}
          <button
            class="quiet small"
            type="button"
            popovertarget="site-domain"
            aria-label="Move the site to {domain}">Move</button
          >
        {/if}
      </span>
    </li>
  </ul>
  <FormDialog id="site-domain" title="Move the site to your domain" action="?/domain" submit="Move">
    {#snippet intro()}Add A and AAAA records for <span class="mono">{domain}</span> and
      <span class="mono">www.{domain}</span>, with the same addresses as
      <span class="mono">{host}</span>. Once they work, your site moves to
      <span class="mono">{domain}</span>, and <span class="mono">{address}</span> sends visitors there.{/snippet}
    <input type="hidden" name="domain" value={domain} />
  </FormDialog>
{/if}
