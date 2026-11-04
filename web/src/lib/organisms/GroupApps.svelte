<script lang="ts">
	import Empty from '#lib/molecules/Empty.svelte';
	/** What acts as the group: each grant, whose it is, to end; and what grants did lately. */
	type Session = { tokenId: string; clientId: string; member: string; source: string; scope?: string; lastUsedAt?: string };
	let { sessions, activity }: { sessions: Session[]; activity: { member: string; clientId?: string; action: string; at: string }[] } = $props();
</script>

<div class="stack">
	{#if sessions.length}
		<ul class="sessions">
			{#each sessions as s (s.tokenId)}
				<li>
					<span class="mono">{s.clientId}</span> <span class="mute">{s.source}, by {s.member}{s.lastUsedAt ? `, last used ${s.lastUsedAt}` : ''}</span>
					<form method="post" action="?/revoke"><input type="hidden" name="tokenId" value={s.tokenId} /><button class="quiet" type="submit">End {s.clientId}'s access</button></form>
				</li>
			{/each}
		</ul>
	{:else}
		<Empty title="No app acts as this group" />
	{/if}
	{#if activity.length}
		<h3>Lately</h3>
		<ul class="activity">
			{#each activity as a, i (i)}<li><span class="mute">{a.at}</span> {a.action}</li>{/each}
		</ul>
	{/if}
</div>
