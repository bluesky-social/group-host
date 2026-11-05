<script lang="ts">
	import ConfirmDialog from '#lib/molecules/ConfirmDialog.svelte';
	import Empty from '#lib/molecules/Empty.svelte';
	import { day } from '#lib/dates.ts';
	/** What acts as the group: each grant, whose it is and how it came, to end, asked to be sure of. */
	type Session = { tokenId: string; clientId: string; member: string; source: string; scope?: string; lastUsedAt?: string };
	let { sessions }: { sessions: Session[] } = $props();
	// An app by its address's host, as people know it, past the scheme.
	const app = (id: string) => id.replace(/^https?:\/\//, '').replace(/\/client-metadata\.json$|\/$/, '');
	// A page of them at a time, more as asked for.
	let shown = $state(5);
</script>

{#if sessions.length}
	<ul class="sessions">
		{#each sessions.slice(0, shown) as s, i (s.tokenId)}
			<li>
				<span class="row-main">
					<strong class="mono">{app(s.clientId)}</strong>
					<span class="what">By {s.source}, for <span class="mono">{s.member}</span>{s.lastUsedAt ? `; last used ${day(s.lastUsedAt)}` : ''}</span>
				</span>
				<span class="row-actions"><button class="quiet small" type="button" popovertarget={`revoke-${i}`} aria-label={`End ${s.clientId}'s access`}>End access</button></span>
				<ConfirmDialog id={`revoke-${i}`} title="End this app's access?" action="?/revoke" confirm="End its access" fields={{ tokenId: s.tokenId }}>
					{app(s.clientId)} can no longer act as the group. To act as it again, it signs in again.
				</ConfirmDialog>
			</li>
		{/each}
	</ul>
	{#if sessions.length > shown}<button class="quiet small" type="button" onclick={() => (shown += 5)}>Show more</button>{/if}
{:else}
	<Empty title="No app acts as this group">An app signed in as the group, by a member who may, shows here.</Empty>
{/if}
