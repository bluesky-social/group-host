<script lang="ts">
  import { enhance } from '$app/forms';
  import Empty from '#lib/molecules/Empty.svelte';
  import FormDialog from '#lib/molecules/FormDialog.svelte';
  import { inPlace } from '#lib/forms.ts';
  import { m } from '#lib/paraglide/messages.js';
  import { ParaglideMessage } from '@inlang/paraglide-js-svelte';
  /**
   * Where a group is served, from its own domain's side: with no `domain`
   * of its own (its handle is its `address`), the way to take one, by the
   * dialog opened by a button whose `popovertarget` is `use-domain`; with
   * one, whether the group is served there (`on`), to move it there, by
   * the dialog opened by a button whose `popovertarget` is `group-domain`,
   * or back. `host` is the name whose addresses the domain's DNS records
   * point at.
   */
  let {
    address,
    domain,
    on,
    host
  }: { address: string; domain: string | null; on: boolean; host: string } = $props();
</script>

{#if !domain}
  <Empty title={m.group_domain_empty_title()}>
    {m.group_domain_empty_body()}
    {#snippet action()}<button class="quiet small" type="button" popovertarget="use-domain"
        >{m.group_domain_change_handle()}</button
      >{/snippet}
  </Empty>
{:else}
  <ul class="domains">
    <li>
      <span class="row-main">
        <strong class="mono">{domain}</strong>
        {#if on}
          <span class="what"
            ><ParaglideMessage message={m.group_domain_on}>
              {#snippet domainName()}<span class="mono">{domain}</span>{/snippet}
              {#snippet addressName()}<span class="mono">{address}</span>{/snippet}
            </ParaglideMessage></span
          >
        {:else}
          <span class="what"
            ><ParaglideMessage message={m.group_domain_off}>
              {#snippet addressName()}<span class="mono">{address}</span>{/snippet}
            </ParaglideMessage></span
          >
        {/if}
      </span>
      <span class="row-actions">
        {#if on}
          <form method="post" action="?/undomain" use:enhance={inPlace}>
            <button class="quiet small" type="submit" aria-label={m.group_domain_move_back_label()}
              >{m.group_domain_move_back()}</button
            >
          </form>
        {:else}
          <button
            class="quiet small"
            type="button"
            popovertarget="group-domain"
            aria-label={m.group_domain_move_label({ domain })}>{m.group_domain_move()}</button
          >
        {/if}
      </span>
    </li>
  </ul>
  <FormDialog
    id="group-domain"
    title={m.group_domain_move_title()}
    action="?/domain"
    submit={m.group_domain_move()}
  >
    {#snippet intro()}<ParaglideMessage message={m.group_domain_move_intro}>
        {#snippet domainName()}<span class="mono">{domain}</span>{/snippet}
        {#snippet wwwName()}<span class="mono">www.{domain}</span>{/snippet}
        {#snippet hostName()}<span class="mono">{host}</span>{/snippet}
        {#snippet addressName()}<span class="mono">{address}</span>{/snippet}
      </ParaglideMessage>{/snippet}
    <input type="hidden" name="domain" value={domain} />
  </FormDialog>
{/if}
