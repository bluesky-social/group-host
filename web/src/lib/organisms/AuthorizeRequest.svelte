<script lang="ts">
  import Avatar from '#lib/atoms/Avatar.svelte';
  import Dialog from '#lib/molecules/Dialog.svelte';
  import Empty from '#lib/molecules/Empty.svelte';
  import GroupRow from '#lib/molecules/GroupRow.svelte';
  import Permissions from '#lib/molecules/Permissions.svelte';
  import { permissions, warnings, type Label } from '#lib/permissions.ts';
  import { appLogo } from '#lib/images.ts';
  import { intl } from '#lib/locale.ts';
  import { m } from '#lib/paraglide/messages.js';
  import { ParaglideMessage } from '@inlang/paraglide-js-svelte';
  type Group = {
    did: string;
    handle: string;
    displayName?: string;
    roles: { role: string }[];
    granted: { scope: string }[];
    withheld: { scope: string }[];
  };
  /**
   * An app's request to sign in as a group: who it is, and for each group
   * the person may sign it in as, what it would get and what it would not,
   * said as an account's PDS says it; approved for one, or declined. The
   * scopes as asked are behind "Technical details". An AI app's, an MCP
   * client's (`mcp`), asks to act as the person in a group instead, with
   * what their roles there allow.
   */
  let {
    client,
    groups,
    labels,
    requested = [],
    requestUri,
    mcp = false
  }: {
    client: { clientId: string; name?: string; uri?: string; logo?: string };
    groups: Group[];
    labels: Label[];
    requested?: { scope: string }[];
    requestUri: string;
    mcp?: boolean;
  } = $props();
  const name = $derived(client.name ?? client.clientId);
  const scopes = (list: { scope: string }[]) => list.map((s) => s.scope);
  // Only `atproto`: it would learn which group it is, and nothing more.
  const bare = (list: { scope: string }[]) => list.every((s) => s.scope === 'atproto');
  // The request stays in the address the form is sent to, for the page
  // drawn after it.
  const action = (name: string) => `?request_uri=${encodeURIComponent(requestUri)}&/${name}`;
  // A group's roles, listed as the reader lists things.
  const roles = (g: Group) =>
    new Intl.ListFormat(intl(), { type: 'unit', style: 'short' }).format(
      g.roles.map((r) => r.role)
    );
</script>

<div class="stack">
  {#if mcp}
    <p>
      <ParaglideMessage message={m.authorize_request_mcp_wants}>
        {#snippet app()}<strong>{name}</strong>{/snippet}
      </ParaglideMessage>
    </p>
    <p class="mute">
      <ParaglideMessage message={m.authorize_request_mcp_unchecked}>
        {#snippet origin()}<span class="mono">{client.clientId}</span>{/snippet}
      </ParaglideMessage>
    </p>
  {:else}
    <div class="app-asks">
      <span class="app-who">
        <Avatar app size="md" src={appLogo(client)} {name} />
        <p>
          <ParaglideMessage message={m.authorize_request_wants}>
            {#snippet app()}<strong>{name}</strong>{/snippet}
          </ParaglideMessage>
        </p>
      </span>
      {#if requested.length}<button class="quiet small" type="button" popovertarget="asked"
          >{m.authorize_request_details()}</button
        >{/if}
    </div>
    <Dialog id="asked" title={m.authorize_request_details()}>
      <p class="mute">{m.authorize_request_scopes()}</p>
      <pre class="well scopes">{scopes(requested).join('\n')}</pre>
    </Dialog>
  {/if}
  {#if client.uri}<p class="mute"><a href={client.uri}>{client.uri}</a></p>{/if}
  {#if groups.length}
    {#each groups as g, n (g.did)}
      <form class="stack" method="post" action={action('approve')}>
        <input type="hidden" name="requestUri" value={requestUri} />
        <input type="hidden" name="group" value={g.did} />
        <GroupRow name={g.displayName ?? g.handle} handle={g.handle} />
        {#if g.roles.length && mcp}
          <p>{m.authorize_request_mcp_roles({ roles: roles(g) })}</p>
          <button type="submit"
            >{m.authorize_request_mcp_connect({ group: g.displayName ?? g.handle })}</button
          >
        {:else if g.roles.length}
          {#if bare(g.granted)}
            <p>{m.authorize_request_bare({ group: g.displayName ?? g.handle })}</p>
          {:else}
            <Permissions id="granted-{n}" list={permissions(scopes(g.granted), labels, g)} />
          {/if}
          {#each warnings(scopes(g.granted)) as w (w)}<p class="danger">{w}</p>{/each}
          {#if g.withheld.length}
            <div class="stack withheld-part">
              <h3>{m.authorize_request_withheld()}</h3>
              <Permissions
                id="withheld-{n}"
                list={permissions(scopes(g.withheld), labels, g)}
                withheld
              />
            </div>
          {/if}
          <button type="submit"
            >{m.authorize_request_sign_in({ group: g.displayName ?? g.handle })}</button
          >
        {:else}
          <p class="mute">{m.authorize_request_no_role()}</p>
        {/if}
      </form>
    {/each}
  {:else}
    <Empty title={m.authorize_request_empty_title()}>{m.authorize_request_empty_body()}</Empty>
  {/if}
  <form method="post" action={action('decline')}>
    <input type="hidden" name="requestUri" value={requestUri} />
    <button class="quiet" type="submit">{m.common_decline()}</button>
  </form>
</div>
