<script lang="ts">
	import ConfirmDialog from '#lib/molecules/ConfirmDialog.svelte';
	import Empty from '#lib/molecules/Empty.svelte';
	import { day } from '#lib/dates.ts';
	/** The AI apps the person connected to the group, to act as them there: each to disconnect, asked to be sure of. */
	type App = { clientId: string; name: string; connectedAt: string; usedAt: string };
	let { apps }: { apps: App[] } = $props();
</script>

{#if apps.length}
	<ul class="ai-apps">
		{#each apps as a, i (a.clientId)}
			<li>
				<span class="row-main"><strong>{a.name}</strong><span class="what">Connected {day(a.connectedAt)}, last used {day(a.usedAt)}</span></span>
				<span class="row-actions"><button class="quiet small" type="button" popovertarget={`disconnect-${i}`}>Disconnect</button></span>
				<ConfirmDialog id={`disconnect-${i}`} title="Disconnect this app?" action="?/disconnect" confirm="Disconnect" fields={{ clientId: a.clientId }}>
					{a.name} can no longer act for you here. You can connect it again later.
				</ConfirmDialog>
			</li>
		{/each}
	</ul>
{:else}
	<Empty title="No AI app connected yet">Add the address above to an AI app, and it shows up here.</Empty>
{/if}
