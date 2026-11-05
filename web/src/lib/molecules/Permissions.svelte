<script lang="ts">
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
	let { id, list, withheld = false }: { id: string; list: Permission[]; withheld?: boolean } = $props();
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
	const anywhere = (owner: string) => (owner === '*' ? 'the network' : owner);
</script>

{#snippet nsid(name: string)}
	<code>{#each name.split('.') as part, i (i)}{#if i}.<wbr />{/if}{part}{/each}</code>
{/snippet}

{#snippet ticks(data: Data[])}
	<table class="ticks">
		<thead><tr><th>Data</th><th>Create</th><th>Update</th><th>Delete</th></tr></thead>
		<tbody>
			{#each data as d (d.collection)}
				<tr>
					<td>{#if d.collection === '*'}<em class="mute">Any collection</em>{:else}{@render nsid(d.collection)}{/if}</td>
					{#each [d.ticks.create, d.ticks.update, d.ticks.delete] as on, i (i)}
						<td>{#if on}<Icon name="check" />{/if}</td>
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
				<div class="row-actions"><button class="quiet" type="button" popovertarget="{id}-{i}">Details</button></div>
				<Dialog id="{id}-{i}" title={p.title}>
					{#if withheld}<p>The app asked for this, and won't get it.</p>{/if}
					{#if p.about}<p class="mute">{p.about}</p>{/if}
					{#if p.data?.length}{@render ticks(p.data)}{/if}
					{#if p.files}<p>The app could upload files too.</p>{/if}
					{#if p.calls?.length}
						<table class="ticks calls">
							<thead><tr><th>Call</th><th>On</th></tr></thead>
							<tbody>
								{#each p.calls as c (c.method + c.service)}
									<tr><td>{#if c.method === '*'}<em class="mute">Any method</em>{:else}{@render nsid(c.method)}{/if}</td><td>{c.service}</td></tr>
								{/each}
							</tbody>
						</table>
					{/if}
					{#each p.spaces ?? [] as s (s.type + s.owner)}
						<div class="space-access">
							<p>
								{#if s.type === '*'}All spaces on <strong>{anywhere(s.owner)}</strong>{:else}{@render nsid(s.type)} spaces on <strong>{anywhere(s.owner)}</strong>{/if}
							</p>
							{#if s.manage}<p class="mute small">Manage the spaces, too: create them, and add and remove members.</p>{/if}
							{#if !s.read && s.readSelf}<p class="mute small">Read only its own data.</p>{/if}
							{#if s.data.length}{@render ticks(s.data)}{:else if s.read && !s.manage}<p class="mute small">Read only.</p>{:else if !s.read && !s.readSelf && !s.manage}<p class="mute small">No access.</p>{/if}
						</div>
					{/each}
				</Dialog>
			{/if}
		</li>
	{/each}
</ul>
