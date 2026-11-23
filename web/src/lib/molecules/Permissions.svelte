<script lang="ts">
  import { ParaglideMessage } from '@inlang/paraglide-js-svelte';
  import { m } from '#lib/paraglide/messages.js';
  import Icon from '#lib/atoms/Icon.svelte';
  import Dialog from '#lib/molecules/Dialog.svelte';
  import type { ComponentProps } from 'svelte';
  import type { Data, Kind, Permission } from '#lib/permissions.ts';
  /**
   * What an app would be let do, a row for each kind of thing, as an
   * account's PDS lists it: its icon, its title, a line on it, and its
   * details in a dialog. `id` keeps the dialogs of two lists apart;
   * `withheld` draws the rows of what it would not get, quieter.
   */
  let {
    id,
    list,
    withheld = false
  }: { id: string; list: Permission[]; withheld?: boolean } = $props();
  const icons: Record<Kind, ComponentProps<typeof Icon>['name']> = {
    email: 'mail',
    identity: 'id-card',
    account: 'user',
    bluesky: 'bluesky',
    chat: 'chat',
    set: 'set',
    repository: 'book',
    authenticate: 'badge',
    spaces: 'users'
  };
  const detailed = (p: Permission) => !!(p.data?.length || p.calls?.length || p.spaces?.length);
  const anywhere = (owner: string) => (owner === '*' ? m.permissions_the_network() : owner);
</script>

{#snippet nsid(name: string)}
  <code
    >{#each name.split('.') as part, i (i)}{#if i}.<wbr />{/if}{part}{/each}</code
  >
{/snippet}

{#snippet ticks(data: Data[])}
  <table class="ticks">
    <thead
      ><tr
        ><th>{m.permissions_data()}</th><th>{m.common_create()}</th><th>{m.permissions_update()}</th
        ><th>{m.common_delete()}</th></tr
      ></thead
    >
    <tbody>
      {#each data as d (d.collection)}
        <tr>
          <td
            >{#if d.collection === '*'}<em class="mute">{m.permissions_any_collection()}</em
              >{:else}{@render nsid(d.collection)}{/if}</td
          >
          {#each [d.ticks.create, d.ticks.update, d.ticks.delete] as on, i (i)}
            <td
              >{#if on}<Icon name="check" />{/if}</td
            >
          {/each}
        </tr>
      {/each}
    </tbody>
  </table>
{/snippet}

<ul class="permissions" class:withheld>
  {#each list as p, i (i)}
    <li>
      <Icon name={icons[p.kind]} lg />
      <div class="row-main">
        <span class="who">{p.title}</span>
        <span class="what">{p.says}</span>
      </div>
      {#if detailed(p)}
        <div class="row-actions">
          <button class="quiet" type="button" popovertarget="{id}-{i}"
            >{m.permissions_details()}</button
          >
        </div>
        <Dialog id="{id}-{i}" title={p.title}>
          {#if withheld}<p>{m.permissions_withheld()}</p>{/if}
          {#if p.about}<p class="mute">{p.about}</p>{/if}
          {#if p.data?.length}{@render ticks(p.data)}{/if}
          {#if p.files}<p>{m.permissions_files()}</p>{/if}
          {#if p.calls?.length}
            <table class="ticks calls">
              <thead><tr><th>{m.permissions_call()}</th><th>{m.permissions_on()}</th></tr></thead>
              <tbody>
                {#each p.calls as c (c.method + c.service)}
                  <tr
                    ><td
                      >{#if c.method === '*'}<em class="mute">{m.permissions_any_method()}</em
                        >{:else}{@render nsid(c.method)}{/if}</td
                    ><td>{c.service}</td></tr
                  >
                {/each}
              </tbody>
            </table>
          {/if}
          {#each p.spaces ?? [] as s (s.type + s.owner)}
            <div class="space-access">
              {#snippet owner()}<strong>{anywhere(s.owner)}</strong>{/snippet}
              <p>
                {#if s.type === '*'}
                  <ParaglideMessage message={m.permissions_all_spaces_on} {owner} />
                {:else}
                  <ParaglideMessage message={m.permissions_spaces_on} {owner}>
                    {#snippet type()}{@render nsid(s.type)}{/snippet}
                  </ParaglideMessage>
                {/if}
              </p>
              {#if s.manage}<p class="mute small">
                  {m.permissions_manage()}
                </p>{/if}
              {#if !s.read && s.readSelf}<p class="mute small">{m.permissions_read_self()}</p>{/if}
              {#if s.data.length}{@render ticks(s.data)}{:else if s.read && !s.manage}<p
                  class="mute small"
                >
                  {m.permissions_read_only()}
                </p>{:else if !s.read && !s.readSelf && !s.manage}<p class="mute small">
                  {m.permissions_no_access()}
                </p>{/if}
            </div>
          {/each}
        </Dialog>
      {/if}
    </li>
  {/each}
</ul>
