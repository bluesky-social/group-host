<script lang="ts">
  import ConfirmDialog from '#lib/molecules/ConfirmDialog.svelte';
  import Empty from '#lib/molecules/Empty.svelte';
  import { day } from '#lib/dates.ts';
  import { m } from '#lib/paraglide/messages.js';
  /** The AI apps the person connected to the group, to act as them there: each to disconnect, asked to be sure of. */
  type App = { clientId: string; name: string; connectedAt: string; usedAt: string };
  let { apps }: { apps: App[] } = $props();
</script>

{#if apps.length}
  <ul class="ai-apps">
    {#each apps as a, i (a.clientId)}
      <li>
        <span class="row-main"
          ><strong>{a.name}</strong><span class="what"
            >{m.connected_apps_when({ connected: day(a.connectedAt), used: day(a.usedAt) })}</span
          ></span
        >
        <span class="row-actions"
          ><button class="quiet small" type="button" popovertarget={`disconnect-${i}`}
            >{m.connected_apps_disconnect()}</button
          ></span
        >
        <ConfirmDialog
          id={`disconnect-${i}`}
          title={m.connected_apps_disconnect_title()}
          action="?/disconnect"
          confirm={m.connected_apps_disconnect()}
          fields={{ clientId: a.clientId }}
        >
          {m.connected_apps_disconnect_body({ app: a.name })}
        </ConfirmDialog>
      </li>
    {/each}
  </ul>
{:else}
  <Empty title={m.connected_apps_empty_title()}>{m.connected_apps_empty_body()}</Empty>
{/if}
