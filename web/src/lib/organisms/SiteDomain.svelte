<script lang="ts">
  import { enhance } from '$app/forms';
  import Empty from '#lib/molecules/Empty.svelte';
  import FormDialog from '#lib/molecules/FormDialog.svelte';
  import { inPlace } from '#lib/forms.ts';
  import { m } from '#lib/paraglide/messages.js';
  import { ParaglideMessage } from '@inlang/paraglide-js-svelte';
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
  <Empty title={m.site_domain_empty_title()}>
    {m.site_domain_empty_body()}
    {#snippet action()}<a class="button quiet small" href={profile}
        >{m.site_domain_change_handle()}</a
      >{/snippet}
  </Empty>
{:else}
  <ul class="sites">
    <li>
      <span class="row-main">
        <strong class="mono">{domain}</strong>
        {#if on}
          <span class="what"
            ><ParaglideMessage message={m.site_domain_on}>
              {#snippet domainName()}<span class="mono">{domain}</span>{/snippet}
              {#snippet addressName()}<span class="mono">{address}</span>{/snippet}
            </ParaglideMessage></span
          >
        {:else}
          <span class="what"
            ><ParaglideMessage message={m.site_domain_off}>
              {#snippet addressName()}<span class="mono">{address}</span>{/snippet}
            </ParaglideMessage></span
          >
        {/if}
      </span>
      <span class="row-actions">
        {#if on}
          <form method="post" action="?/undomain" use:enhance={inPlace}>
            <button class="quiet small" type="submit" aria-label={m.site_domain_move_back_label()}
              >{m.site_domain_move_back()}</button
            >
          </form>
        {:else}
          <button
            class="quiet small"
            type="button"
            popovertarget="site-domain"
            aria-label={m.site_domain_move_label({ domain })}>{m.site_domain_move()}</button
          >
        {/if}
      </span>
    </li>
  </ul>
  <FormDialog
    id="site-domain"
    title={m.site_domain_move_title()}
    action="?/domain"
    submit={m.site_domain_move()}
  >
    {#snippet intro()}<ParaglideMessage message={m.site_domain_move_intro}>
        {#snippet domainName()}<span class="mono">{domain}</span>{/snippet}
        {#snippet wwwName()}<span class="mono">www.{domain}</span>{/snippet}
        {#snippet hostName()}<span class="mono">{host}</span>{/snippet}
        {#snippet addressName()}<span class="mono">{address}</span>{/snippet}
      </ParaglideMessage>{/snippet}
    <input type="hidden" name="domain" value={domain} />
  </FormDialog>
{/if}
