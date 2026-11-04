<script lang="ts">
	import Empty from '#lib/molecules/Empty.svelte';
	type Member = { actor: { did: string; handle?: string }; roles: { role: string }[] };
	/**
	 * Who is in the group, and their roles: for one who may give roles, the
	 * ones they may give, to give or take; for one who may put people out,
	 * the way to. Their own row they leave from, not here.
	 */
	let {
		members,
		me,
		assignable = [],
		canEject = false
	}: { members: Member[]; me?: string; assignable?: string[]; canEject?: boolean } = $props();
	const named = (m: Member) => (m.actor.handle ? `@${m.actor.handle}` : m.actor.did);
</script>

{#if members.length}
	<ul class="members">
		{#each members as m (m.actor.did)}
			<li class="member">
				<span class="mono">{named(m)}</span>
				<span class="mute">{m.roles.map((r) => r.role).join(', ')}</span>
				{#if m.actor.did !== me && assignable.length}
					<form method="post" action="?/roles">
						<input type="hidden" name="did" value={m.actor.did} />
						{#each m.roles.filter((r) => !assignable.includes(r.role)) as kept (kept.role)}
							<input type="hidden" name="role" value={kept.role} />
						{/each}
						{#each assignable as role (role)}
							<label><input type="checkbox" name="role" value={role} checked={m.roles.some((r) => r.role === role)} /> {role}</label>
						{/each}
						<button class="quiet" type="submit">Save {named(m)}'s roles</button>
					</form>
				{/if}
				{#if m.actor.did !== me && canEject}
					<form method="post" action="?/eject">
						<input type="hidden" name="did" value={m.actor.did} />
						<button class="quiet" type="submit">Put {named(m)} out</button>
					</form>
				{/if}
			</li>
		{/each}
	</ul>
{:else}
	<Empty title="No members yet" />
{/if}
