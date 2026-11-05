<script lang="ts">
  import Dialog from '#lib/molecules/Dialog.svelte';
  import Empty from '#lib/molecules/Empty.svelte';
  import GroupRow from '#lib/molecules/GroupRow.svelte';
  import Permissions from '#lib/molecules/Permissions.svelte';
  import { permissions, warnings, type Label } from '#lib/permissions.ts';
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
    client: { clientId: string; name?: string; uri?: string };
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
</script>

<div class="stack">
  {#if mcp}
    <p>
      An AI app calling itself <strong>{name}</strong> wants to act as you in one of your groups.
    </p>
    <p class="mute">
      The app chose this name, and it isn't checked. It came from <span class="mono"
        >{client.clientId}</span
      >.
    </p>
  {:else}
    <div class="app-asks">
      <p><strong>{name}</strong> wants to sign in as one of your groups.</p>
      {#if requested.length}<button class="quiet small" type="button" popovertarget="asked"
          >Technical details</button
        >{/if}
    </div>
    <Dialog id="asked" title="Technical details">
      <p class="mute">The app asks for these permissions (scopes):</p>
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
          <p>
            It could do what your roles there let you do ({g.roles.map((r) => r.role).join(', ')}).
          </p>
          <button type="submit">Connect it to {g.displayName ?? g.handle}</button>
        {:else if g.roles.length}
          {#if bare(g.granted)}
            <p>The app would only know it's signed in as {g.displayName ?? g.handle}.</p>
          {:else}
            <Permissions id="granted-{n}" list={permissions(scopes(g.granted), labels, g)} />
          {/if}
          {#each warnings(scopes(g.granted)) as w (w)}<p class="danger">{w}</p>{/each}
          {#if g.withheld.length}
            <div class="stack withheld-part">
              <h3>It would not get</h3>
              <Permissions
                id="withheld-{n}"
                list={permissions(scopes(g.withheld), labels, g)}
                withheld
              />
            </div>
          {/if}
          <button type="submit">Sign it in as {g.displayName ?? g.handle}</button>
        {:else}
          <p class="mute">
            You don't have a role in this group, so you can't sign an app in as it.
          </p>
        {/if}
      </form>
    {/each}
  {:else}
    <Empty title="No group you can sign it in as"
      >You don't have a role in any group on this host.</Empty
    >
  {/if}
  <form method="post" action={action('decline')}>
    <input type="hidden" name="requestUri" value={requestUri} />
    <button class="quiet" type="submit">Decline</button>
  </form>
</div>
